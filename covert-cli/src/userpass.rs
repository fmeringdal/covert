use clap::{Args, Subcommand};
use covert_sdk::{
    userpass::{CreateUserParams, UpdateUserPasswordParams},
    Client,
};

use crate::handle_resp;

#[derive(Args, Debug)]
pub struct Userpass {
    #[clap(subcommand)]
    subcommand: UserpassSubcommand,
}

#[derive(Subcommand, Debug)]
pub enum UserpassSubcommand {
    #[command(about = "add user")]
    Add {
        #[arg(help = "username of user to create")]
        username: String,
        #[arg(short, long)]
        password: String,
        #[arg(short, long)]
        path: String,
    },
    #[command(about = "remove user")]
    Remove {
        #[arg(help = "username of user to remove")]
        username: String,
        #[arg(short, long)]
        path: String,
    },
    #[command(about = "list users")]
    List {
        #[arg(help = "path of the userpass auth method")]
        path: String,
    },
    #[command(about = "update password for user")]
    UpdatePassword {
        #[arg(help = "username of user to update password for")]
        username: String,
        #[arg(short, long)]
        password: String,
        #[arg(long)]
        new_password: String,
        #[arg(short, long)]
        path: String,
    },
}

impl Userpass {
    pub async fn handle(self, sdk: &Client) {
        match self.subcommand {
            UserpassSubcommand::Add {
                username,
                password,
                path,
            } => {
                let resp = sdk
                    .userpass
                    .create(&path, &CreateUserParams { username, password })
                    .await;
                handle_resp(resp);
            }
            UserpassSubcommand::List { path } => {
                let resp = sdk.userpass.list(&path).await;
                handle_resp(resp);
            }
            UserpassSubcommand::Remove { path, username } => {
                let resp = sdk.userpass.remove(&path, &username).await;
                handle_resp(resp);
            }
            UserpassSubcommand::UpdatePassword {
                path,
                username,
                password,
                new_password,
            } => {
                let resp = sdk
                    .userpass
                    .update_password(
                        &path,
                        &username,
                        &UpdateUserPasswordParams {
                            password,
                            new_password,
                        },
                    )
                    .await;
                handle_resp(resp);
            }
        }
    }
}
