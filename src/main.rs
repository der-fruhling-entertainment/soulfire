use std::{collections::{BTreeMap, HashMap}, ops::{Range, RangeBounds}, fs, env};

use lazy_static::lazy_static;
use rocket::{get, serde::json::Json, routes, fs::{FileServer, NamedFile}, response::Redirect, http::{CookieJar, Cookie, Status}, State, futures::TryFutureExt, FromForm, post, form::Form};
use rocket_dyn_templates::{Template, context};
use serde::{Serialize, Deserialize};
use soulfire::*;

lazy_static! {
    static ref GAMES: HashMap<String, (Game, GameInfo)> = load_games();
}

fn load_games() -> HashMap<String, (Game, GameInfo)> {
    let mut map = HashMap::default();
    
    for game in fs::read_dir("games").unwrap().filter_map(Result::ok) {
        if game.file_type().unwrap().is_file() {
            let contents = fs::read_to_string(game.path()).unwrap();
            let yaml: Game = serde_yml::from_str(&contents).unwrap_or_else(|e| panic!("failed to parse game {:?}: {e}", game.path()));
            
            let path = game.path().with_extension("");
            let name = path.file_name().unwrap().to_string_lossy();
            let info = GameInfo::from_suffix(&yaml.suffix);
            map.insert(name.as_ref().to_owned(), (yaml, info));
        }
    }
    
    map
}

struct BotInfo {
    domain: String,
    client: reqwest::Client
}

struct GameInfo {
    application_id: u64,
    client_id: u64,
    client_secret: String,
}

impl GameInfo {
    pub fn from_suffix(suffix: &str) -> Self {
        Self {
            application_id: u64::from_str_radix(&env::var(format!("APP_ID_{suffix}")).unwrap(), 10).unwrap(),
            client_id: u64::from_str_radix(&env::var(format!("CLIENT_ID_{suffix}")).unwrap(), 10).unwrap(),
            client_secret: env::var(format!("CLIENT_SECRET_{suffix}")).unwrap(),
        }
    }
}

#[rocket::launch]
fn launch() -> _ {
    #[allow(unused_mut)]
    let mut rk = rocket::build()
        .attach(Template::fairing())
        .manage(BotInfo {
            domain: env::var("DOMAIN").unwrap_or_else(|_| "soulfire.derfrühling.net".to_string()),
            client: reqwest::Client::builder()
                .build().unwrap()
        })
        .mount("/", routes![get_game, get_game_link_page, set_game_link_status, get_link_success, link_discord, add_bot, get_all_games]);

    #[cfg(feature = "assets-hosting")] {
        rk = rk.mount("/assets", FileServer::from("assets/"));
    }

    rk
}

#[derive(rocket::Responder)]
enum Error {
    #[response(status = 404)]
    NotFound(&'static str),
    #[response(status = 400)]
    BadRequest(&'static str),
    #[response(status = 500)]
    InternalServerError(&'static str),
    DiscordPassed((Status, String))
}

#[get("/games/<game>")]
fn get_game(game: &str) -> Result<Json<Game>, Error> {
    match GAMES.get(game) {
        Some((v, _)) => Ok(Json(v.clone())),
        None => Err(Error::NotFound("The requested game was not found.")),
    }
}

#[get("/games/<game>/link?<hpn>")]
fn get_game_link_page(game: &str, hpn: Option<bool>, bot: &State<BotInfo>) -> Result<Template, Error> {
    match GAMES.get(game) {
        Some((v, _)) => Ok(Template::render("entry", context! {
            id: game,
            domain: &bot.domain,
            name: &v.name,
            uid_max_length: v.uid.max_length,
            username: context! {
                is_optional: v.username.optional,
                max_length: v.username.max_length
            },
            hide_privacy_notice: hpn.unwrap_or_default()
        })),
        None => Err(Error::NotFound("The requested game was not found.")),
    }
}

#[derive(FromForm)]
struct GameLinkStatus {
    uid: u64,
    username: String
}

#[post("/games/<game>/link", data = "<data>")]
async fn set_game_link_status(game: &str, data: Form<GameLinkStatus>, jar: &CookieJar<'_>, bot: &State<BotInfo>) -> Result<Redirect, Error> {
    match GAMES.get(game) {
        Some((v, info)) => {
            let cookie = jar.get("dstk").ok_or_else(|| Error::BadRequest("No token acquired."))?;
            let token = decrypt_key(cookie.value()).map_err(|_| Error::BadRequest("Invalid token"))?;
            
            let res = bot.client
                .put(format!("https://discord.com/api/v10/users/@me/applications/{}/role-connection", info.application_id))
                .body(serde_json::to_string(&v.make_role_connection_info(data.uid, &data.username))
                    .map_err(|_| Error::InternalServerError("Internal server error. Oops!"))?)
                .header("Content-Type", "application/json")
                .header("User-Agent", "DiscordBot (https://github.com/der-fruhling)")
                .header("Authorization", format!("Bearer {}", token))
                .send().await.map_err(|e| {
                    log::error!("error interacting with Discord to change role connection info: {e}");
                    Error::InternalServerError("Internal server error. Oops!")
                })?;
            
            if !res.status().is_success() {
                log::error!("Failed to set role connection: {} {:?}", res.status(), res.text().await);
                return Err(Error::InternalServerError("Failed to set your role connection.\n-> That's an internal server error. Oops!"));
            }
            
            jar.remove("dstk");
            Ok(Redirect::to(format!("/success")))
        },
        None => Err(Error::NotFound("The requested game was not found.")),
    }
}

#[get("/success")]
fn get_link_success() -> Template {
    Template::render("success", ())
}

#[derive(Deserialize)]
struct TokenReturn<'a> {
    #[serde(borrow)] access_token: &'a str,
    expires_in: usize,
    #[serde(borrow)] scope: &'a str
}

#[get("/games/<game>/discord-auth-flow?<code>")]
async fn link_discord(game: &str, code: &str, jar: &CookieJar<'_>, bot: &State<BotInfo>) -> Result<Redirect, Error> {
    match GAMES.get(game) {
        Some((v, info)) => {
            if code.chars().any(|c| !c.is_alphanumeric()) {
                return Err(Error::BadRequest("Bad request."));
            }
            
            let contents = format!(
                "grant_type=authorization_code&code={code}&redirect_uri={}",
                urlencoding::encode(&format!("https://{}/games/{game}/discord-auth-flow", bot.domain))
            );
            
            let res = bot.client.post("https://discord.com/api/v10/oauth2/token")
                .body(contents)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .header("User-Agent", "DiscordBot (https://github.com/der-fruhling)")
                .basic_auth(info.client_id, Some(&info.client_secret))
                .send().await.map_err(|e| {
                    log::error!("error interacting with Discord for an auth token: {e:?}");
                    Error::InternalServerError("Internal server error. Oops!")
                })?;

            if !res.status().is_success() {
                return Err(Error::DiscordPassed((Status::from_code(res.status().into()).unwrap(), format!("Discord-passed internal error. {}", res.status()))));
            }

            let token_data = res.text().await
                .unwrap_or_else(|_| panic!("successful auth interaction with Discord; no body????"));
            let token_data: TokenReturn = serde_json::from_str(&token_data)
                .map_err(|_| Error::InternalServerError("Internal server error. Oops!"))?;

            if token_data.scope != "role_connections.write" {
                return Err(Error::BadRequest("Invalid operation"));
            }

            jar.add(Cookie::build(("dstk", generate_encrypted_key(token_data.access_token)))
                .secure(true)
                .expires(cookie::Expiration::DateTime(time::OffsetDateTime::now_utc() + time::Duration::seconds((token_data.expires_in - 100) as i64)))
                .same_site(cookie::SameSite::Strict));

            Ok(Redirect::to(format!("/games/{game}/link?hpn")))
        },
        None => Err(Error::NotFound("The requested game was not found.")),
    }
}

#[get("/games/<game>/add-bot")]
async fn add_bot(game: &str) -> Result<Template, Error> {
    match GAMES.get(game) {
        Some((v, info)) => {
            Ok(Template::render("add-bot", context! {
                name: &v.name,
                auth: format!("https://discord.com/oauth2/authorize?client_id={}&permissions=0&scope=bot", info.client_id)
            }))
        },
        None => Err(Error::NotFound("The requested game was not found.")),
    }
}

#[get("/all-games")]
async fn get_all_games() -> Result<Template, Error> {
    Ok(Template::render("all-games", context! {
        games: GAMES.iter()
            .map(|(id, (game, _))| context! {
                name: &game.name,
                id: id,
                main_page: game.main_page.as_ref()
            })
            .collect::<Vec<_>>()
    }))
}
