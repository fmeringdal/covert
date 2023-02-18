use clap::{Args, Subcommand};
use covert_sdk::{
    userpass::{CreateUserParams, LoginParams, UpdateUserPasswordParams},
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
        #[arg(short, long)]
        username: String,
        #[arg(short, long)]
        password: String,
        #[arg(long)]
        path: String,
    },
    #[command(about = "remove user")]
    Remove {
        #[arg(short, long)]
        username: String,
        #[arg(short, long)]
        path: String,
    },
    #[command(about = "list users")]
    List {
        #[arg(help = "path of the userpass auth method")]
        path: String,
    },
    #[command(about = "login")]
    Login {
        #[arg(short, long)]
        username: String,
        #[arg(short, long)]
        password: String,
        #[arg(long)]
        path: String,
    },
    #[command(about = "update password for user")]
    UpdatePassword {
        #[arg(short, long)]
        username: String,
        #[arg(short, long)]
        password: String,
        #[arg(long)]
        new_password: String,
        #[arg(long)]
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
            UserpassSubcommand::Login {
                path,
                username,
                password,
            } => {
                let resp = sdk
                    .userpass
                    .login(&path, &LoginParams { username, password })
                    .await;
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
