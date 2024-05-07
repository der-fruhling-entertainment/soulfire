use std::{fs, env, sync::Arc};
use log4rs::{append::console::ConsoleAppender, config::{Root, Appender}, encode::pattern::PatternEncoder};
use soulfire::{Game, RoleConnectionMetadataRecord};

#[tokio::main]
async fn main() {
    log4rs::init_config(log4rs::Config::builder()
        .appender(Appender::builder()
            .build("console", Box::new(ConsoleAppender::builder()
                .encoder(Box::new(PatternEncoder::new("{h([{l}])} {t} -> {m}{n}")))
                .build())))
        .build(Root::builder().appender("console").build(log::LevelFilter::Info))
        .unwrap()).unwrap();
    
    log::info!(target: "soulfire::configure", "Updating config for all specified games");
    let client = Arc::new(reqwest::Client::default());
    
    for game in fs::read_dir("games").unwrap().filter_map(Result::ok) {
        if game.file_type().unwrap().is_file() {
            let contents = fs::read_to_string(game.path()).unwrap();
            let yaml: Game = serde_yml::from_str(&contents).unwrap_or_else(|e| panic!("failed to parse game {:?}: {e}", game.path()));
            let log_target = format!("soulfire::configure[{}]", game.path().with_extension("").file_name().unwrap_or_default().to_string_lossy());
            log::info!(target: &log_target, "Starting update for {}", &yaml.name);
            
            let bot_token = env::var(format!("BOT_TOKEN_{}", yaml.suffix)).expect("no bot token for a game!");
            let application_id = env::var(format!("APP_ID_{}", yaml.suffix)).expect("no app id for a game!");
            
            let name_1 = yaml.name.clone();
            let name_2 = yaml.name.clone();
            let name = yaml.name.clone();
            
            let client_1 = client.clone();
            let application_id_1 = application_id.clone();
            let bot_token_1 = bot_token.clone();
            let (existing_data, new_data) = tokio::join!(
                tokio::spawn(async move {
                    let client = client_1;
                    let existing_data = client
                        .get(format!("https://discord.com/api/v10/applications/{application_id_1}/role-connections/metadata"))
                        .header("Authorization", format!("Bot {bot_token_1}"))
                        .header("User-Agent", "DiscordBot (https://github.com/der-fruhling)")
                        .send().await.expect("failed to send request")
                        .error_for_status().expect("request rejected")
                        .text().await.expect("failed to read text of existing data");
                    
                    let mut existing_data: Vec<RoleConnectionMetadataRecord> = serde_json::from_str(&existing_data).expect("failed to parse existing data");
                    
                    existing_data.sort();
                    log::debug!("Existing data for {}: {:?}", name_1, existing_data);
                    existing_data
                }),
                tokio::spawn(async move {
                    let mut records = yaml.make_role_connection_records();
                    records.sort();
                    log::debug!("New data for {}: {:?}", name_2, records);
                    records
                })
            );
            
            let (existing_data, new_data) = (existing_data.unwrap(), new_data.unwrap());
            
            if existing_data != new_data {
                log::warn!(target: &log_target, "Existing and new data for {} do not match! Updating...", &name);
                let client = client.clone();
                let json = serde_json::to_string(&new_data).unwrap();
                log::debug!("Sending {:?} as new role connection metadata for {}", &json, &name);
                let res = client
                    .put(format!("https://discord.com/api/v10/applications/{application_id}/role-connections/metadata"))
                    .body(json)
                    .header("Authorization", format!("Bot {bot_token}"))
                    .header("User-Agent", "DiscordBot (https://github.com/der-fruhling)")
                    .header("Content-Type", "application/json")
                    .send().await.expect("failed to send request");
                
                if !res.status().is_success() {
                    let status = res.status();
                    let error = res.text().await.expect("failed to read error text");
                    log::error!(target: &log_target, "{} {} (put): {}", status, &name, error);
                } else {
                    log::info!(target: &log_target, "Updated data for {}", name);
                }
            } else {
                log::info!(target: &log_target, "Game {} up to date", name);
            }
        }
    }
}
