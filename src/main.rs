use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    process,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use twilight_http::Client;
use twilight_model::{
    guild::{Permissions, Role},
    id::{
        marker::{RoleMarker, UserMarker},
        Id,
    },
};

const TOKEN_FILE_PATH: &str = "token.txt";

fn clear_terminal() {
    print!("{}[2J", 27 as char);
}

fn ask_for_input(prompt: &str) -> String {
    let mut buffer = String::new();
    println!("{}", prompt);
    print!("> ");
    io::stdout().flush().unwrap();
    io::stdin().read_line(&mut buffer).unwrap();
    buffer = buffer.lines().next().unwrap().to_string();
    buffer
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Checking for token file...");
    let login_file_exists = match fs::metadata(TOKEN_FILE_PATH) {
        Ok(_) => true,
        Err(_) => false,
    };

    if !login_file_exists {
        let token = ask_for_input("Looks like you don't have a token file.\nPlease enter your bot token below.\nYou can get one from https://discord.com/developers/applications");
        fs::write(TOKEN_FILE_PATH, &token).unwrap();
        clear_terminal();
        println!("Thanks! Saving your token to token.txt");
    } else {
        println!("Token file found!");
    }

    let http = Arc::new(Client::new(fs::read_to_string(TOKEN_FILE_PATH).unwrap()));
    let application_response = http.current_user_application().await;
    let application = match application_response {
        Ok(response) => response.model().await.unwrap(),
        Err(_) => {
            println!("Invalid token provided. Please check your token and try again.");
            process::exit(1);
        }
    };

    println!("Running as {}", application.name);

    let guild_id = ask_for_input("Please enter the ID of the guild you want to use this bot in.");
    let guild_id = match guild_id.parse::<u64>() {
        Ok(id) => Id::new(id),
        Err(_) => {
            println!("Invalid guild ID provided. Please try again.");
            process::exit(1);
        }
    };
    let guild_response = http.guild(guild_id).await;
    let guild = match guild_response {
        Ok(response) => response.model().await.unwrap(),
        Err(_) => {
            println!("Invalid guild ID provided. Please try again.");
            process::exit(1);
        }
    };
    println!("Working at {} guild.", guild.name);

    let guild_roles_vec = http.roles(guild_id).await?.model().await?;
    let guild_roles_map: HashMap<Id<RoleMarker>, &Role> =
        HashMap::from_iter(guild_roles_vec.iter().map(|role| (role.id, role)));

    let has_ban_permission = http
        .guild_member(guild_id, application.id.cast() as Id<UserMarker>)
        .await
        .unwrap()
        .model()
        .await
        .unwrap()
        .roles
        .iter()
        .map(|role_id| guild_roles_map.get(role_id).unwrap())
        .any(|role| role.permissions.contains(Permissions::BAN_MEMBERS));

    if !has_ban_permission {
        println!("The bot does not have permission to unban users. Please add the ban permission to the bot and try again.");
        process::exit(1);
    }

    let unban_count = ask_for_input("Please enter the number of users you want to unban.");
    let unban_count = match unban_count.parse::<u16>() {
        Ok(count) => count,
        Err(_) => {
            println!("Invalid number provided. Please try again.");
            process::exit(1);
        }
    };

    if unban_count > 1000 {
        // if unban_count is greater than 1000, we need to unban in batches of 1000
        let mut last_unban_id: Option<Id<UserMarker>> = None;
        let mut batch_count = (unban_count / 1000) + (unban_count % 1000 != 0) as u16;
        let mut batch_index = 1;
        let mut unbanned_count = 0;
        let actual_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();

        while batch_count > 0 {
            let mut banned_users = http.bans(guild_id);

            if last_unban_id.is_some() {
                banned_users = banned_users.after(last_unban_id.unwrap());
            }

            let banned_users = banned_users.limit(1000).unwrap().await?.model().await?;

            if banned_users.is_empty() {
                println!("No more users to unban.");
                break;
            }

            let batch_file_name = format!(
                "unban_report_{}_batch_{}_{}.txt",
                guild_id, batch_index, actual_ms
            );
            File::create(batch_file_name.clone()).unwrap();
            let batch_file = OpenOptions::new()
                .write(true)
                .append(true)
                .open(batch_file_name.clone())
                .unwrap();

            for ban in banned_users.iter() {
                let user_name = ban
                    .user
                    .global_name
                    .to_owned()
                    .unwrap_or(ban.user.name.to_owned());
                println!(
                    "{} - Unbanning {} ({})",
                    unbanned_count + 1,
                    user_name,
                    ban.user.id
                );

                let unban_result = http.delete_ban(guild_id, ban.user.id).await;
                if unban_result.is_err() {
                    println!(
                        "{} - Error: {}",
                        unbanned_count + 1,
                        unban_result.err().unwrap()
                    );
                    continue;
                }

                writeln!(&batch_file, "{} ({})", user_name, ban.user.id).unwrap();
                unbanned_count += 1;
            }

            last_unban_id = Some(banned_users.last().unwrap().user.id);
            batch_count -= 1;
            batch_index += 1;
        }

        println!("Unbanned {} users.", unbanned_count);
    } else {
        let banned_users = http
            .bans(guild_id)
            .limit(unban_count)
            .unwrap()
            .await?
            .model()
            .await?;

        if banned_users.is_empty() {
            println!("No users to unban.");
        } else {
            let file_name = format!(
                "unban_report_{}_{}.txt",
                guild_id,
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis()
            );

            File::create(file_name.clone()).unwrap();
            let file = OpenOptions::new()
                .write(true)
                .append(true)
                .open(file_name.clone())
                .unwrap();

            let mut unbanned_count = 0;
            for ban in banned_users.iter() {
                let user_name = ban
                    .user
                    .global_name
                    .to_owned()
                    .unwrap_or(ban.user.name.to_owned());
                println!(
                    "{} - Unbanning {} ({})",
                    unbanned_count + 1,
                    user_name,
                    ban.user.id
                );

                let unban_result = http.delete_ban(guild_id, ban.user.id).await;
                if unban_result.is_err() {
                    println!(
                        "{} - Error: {}",
                        unbanned_count + 1,
                        unban_result.err().unwrap()
                    );
                    continue;
                }
                writeln!(&file, "{} ({})", user_name, ban.user.id).unwrap();
                unbanned_count += 1;
            }
            println!("Unbanned {} users.", unbanned_count);
        }
    }

    Ok(())
}
