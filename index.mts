import * as fs from "node:fs";
import * as crypto from "node:crypto";
import {Request, Response} from "@google-cloud/functions-framework";
import {parse} from "cookie";
import fetch from "node-fetch";

const SIMPLE_HTML_FORM: string = fs.readFileSync("form.html").toString();

const APP_ID: string = "1203211791807545455";

const key = crypto
    .createHash('sha512')
    .update(process.env.CIPHER_KEY!)
    .digest('hex')
    .substring(0, 32)
const encryptionIV = crypto
    .createHash('sha512')
    .update(process.env.CIPHER_IV!)
    .digest('hex')
    .substring(0, 16)

function encryptKey(data: string) {
    const cipher = crypto.createCipheriv('aes-256-cbc', key, encryptionIV);
    return Buffer.from(
        cipher.update(data, 'utf8', 'hex') + cipher.final('hex')
    ).toString('base64')
}

function decryptKey(data: string) {
    const buffer = Buffer.from(data, 'base64');
    const decipher = crypto.createDecipheriv('aes-256-cbc', key, encryptionIV);
    return (decipher.update(buffer.toString('utf8'), 'hex', 'utf8') +
        decipher.final('utf8'))
}

export async function validateToken(token: string | null): Promise<boolean> {
    if(token == null) return false;
    
    const response = await fetch(`https://discord.com/api/v10/users/@me/applications/${APP_ID}/role-connection`, {
        method: 'GET',
        headers: {
            'User-Agent': 'DiscordBot (https://github.com/der_fruhling/, 0.1.0)',
            'Authorization': `Bearer ${token}`
        }
    });
    
    return response.ok;
}

interface DiscordOAuthResponse {
    access_token: string
}

export async function invokeLinking(req: Request, res: Response) {
    if(req.method === "GET") {
        if(req.query['code']) {
            const body = new URLSearchParams({
                client_id: "1203211791807545455",
                client_secret: process.env.DISCORD_CLIENT_SECRET,
                grant_type: 'authorization_code',
                code: req.query['code'].toString(),
                redirect_uri: 'https://us-west1-steam-talent-413205.cloudfunctions.net/hi3-validate-linked-roles'
            } as Record<string, string>);

            const response = await fetch("https://discord.com/api/v10/oauth2/token", {
                body,
                method: 'POST',
                headers: {
                    'Content-Type': 'application/x-www-form-urlencoded',
                    'User-Agent': 'DiscordBot (https://github.com/der_fruhling/, 0.1.0)'
                }
            });

            if(response.ok) {
                const data = (await response.json()) as DiscordOAuthResponse;
                res.cookie("dstk", encryptKey(data.access_token), {
                    expires: new Date(Date.now() + 12_000 * 60 * 60),
                    sameSite: 'strict',
                    secure: true
                });
            } else {
                res.status(500)
                    .setHeader("Content-Type", "text/html")
                    .send("Error during authorization flow. <a href=\"https://us-west1-steam-talent-413205.cloudfunctions.net/hi3-validate-linked-roles\">Click here to return to the main page.</a>");
                console.error(`Error accessing Discord OAuth2 API: ${JSON.stringify(await response.json())}`)
                return;
            }
        }
        
        const cookie = req.header("Cookie")?.toString();
        
        if(cookie) {
            const cookies = parse(cookie!);
            const encToken = cookies['dstk'];
            
            if(!await validateToken(encToken)) {
                res.clearCookie('dstk');
            }
        }

        res.setHeader("Content-Type", "text/html");
        res.send(SIMPLE_HTML_FORM);
    } else if (req.method === "POST") {
        if (req.header("Content-Type") != "application/x-www-form-urlencoded" || !req.header("Cookie")) {
            res.sendStatus(400);
            return;
        }

        const uid = req.body['uid'] as string;
        const username = req.body['username'] as string;
        
        if(username.length > 16) {
            res.status(400)
                .setHeader("Content-Type", "text/html")
                .send("Invalid username detected. <a href=\"https://us-west1-steam-talent-413205.cloudfunctions.net/hi3-validate-linked-roles\">Click here to input something else.</a>");
        }
        
        if(uid.length > 10 || !uid.matchAll(/^[0-9]{,16}$/g)) {
            res.status(400)
                .setHeader("Content-Type", "text/html")
                .send("Invalid UID detected. <a href=\"https://us-west1-steam-talent-413205.cloudfunctions.net/hi3-validate-linked-roles\">Click here to input something else.</a>");
        }

        const cookies = parse(req.header("Cookie")?.toString()!!);
        const encToken = cookies['dstk'];
        const token = decryptKey(encToken);

        const region = getHi3Region(Number.parseInt(uid, 10));
        
        const isAsia = region === 'SEA';
        const isNA = region === 'NA';
        const isEU = region === 'EU';
        
        const body = JSON.stringify({
            platform_name: `Honkai Impact 3rd`,
            platform_username: `${username} (${uid})`,
            metadata: {
                is_asia: isAsia ? '1' : '0',
                is_na: isNA ? '1' : '0',
                is_eu: isEU ? '1' : '0',
                is_elsewhere: !(isAsia || isNA || isEU) ? '1' : '0'
            }
        });
        
        if(!(isAsia || isNA || isEU)) {
            console.error(`UID ${uid} is invalid or not recognized`);
        }

        const response = await fetch(`https://discord.com/api/v10/users/@me/applications/${APP_ID}/role-connection`, {
            body,
            method: 'PUT',
            headers: {
                'Content-Type': 'application/json',
                'User-Agent': 'DiscordBot (https://github.com/der_fruhling/, 0.1.0)',
                'Authorization': `Bearer ${token}`
            }
        });
        
        console.log(response.body)
        
        res.clearCookie("dstk");
        
        fetch("https://discord.com/api/v10/oauth2/token/revoke", {
            body: new URLSearchParams({
                client_id: "1203211791807545455",
                client_secret: process.env.DISCORD_CLIENT_SECRET,
                token,
                token_type_hint: 'access_token'
            } as Record<string, string>),
            headers: {
                'Content-Type': 'application/json',
                'User-Agent': 'DiscordBot (https://github.com/der_fruhling/, 0.1.0)'
            }
        }).then(value => {
            if(!value.ok) {
                console.error(`Failed to revoke Discord token! ${value.status} ${value.statusText}: ${value.body}`);
            }
        }).catch(reason => {
            console.error(`Failed to send token revocation request: ${reason}`);
        })

        if (!response.ok) {
            res.status(500)
                .setHeader("Content-Type", "text/html")
                .send("Error while setting role connections. <a href=\"https://us-west1-steam-talent-413205.cloudfunctions.net/hi3-validate-linked-roles\">Click here to return to the main page.</a>");
            console.error(`Error accessing Discord Role Connections API: ${JSON.stringify(await response.json())}`)
        } else {
            res.setHeader("Content-Type", "text/html")
                .send("Success! You can now return to Discord.")
        }
    }
}

// copied from https://github.com/vermaysha/hoyoapi/blob/master/src/client/hi/hi.helper.ts#L4-L27
// MIT License, Copyright (c) 2023 Ashary Vermaysha
// slightly tweaked for visual stuff
function getHi3Region(uid: number) {
    if (uid > 10_000_000 && uid < 100_000_000) {
        return 'SEA'
    } else if (uid > 100_000_000 && uid < 200_000_000) {
        return 'NA'
    } else if (uid > 200_000_000 && uid < 300_000_000) {
        return 'EU'
    } else {
        return 'UNKNOWN'
    }
}
// end copy and paste (ty hoyoapi dev(s))
