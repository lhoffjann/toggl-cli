mod api;
mod arguments;
mod commands;
mod config;
mod constants;
mod credentials;
mod error;
mod models;
mod picker;
mod utilities;

use api::client::ApiClient;
use api::client::V9ApiClient;
use arguments::Command::Auth;
use arguments::Command::Config;
use arguments::Command::Continue;
use arguments::Command::Current;
use arguments::Command::List;
use arguments::Command::Running;
use arguments::Command::Start;
use arguments::Command::Stop;
use arguments::CommandLineArguments;
use arguments::ConfigSubCommand;
use colored::Colorize;
use commands::auth::AuthenticationCommand;
use commands::cont::ContinueCommand;
use commands::list::ListCommand;
use commands::running::RunningTimeEntryCommand;
use commands::start::StartCommand;
use commands::stop::{StopCommand, StopCommandOrigin};
use credentials::{Credentials, CredentialsStorage, KeyringStorage};
use keyring::Entry;
use models::ResultWithDefaultError;
use std::io;
use structopt::StructOpt;

#[tokio::main]
async fn main() -> ResultWithDefaultError<()> {
    let parsed_args = CommandLineArguments::from_args();
    match execute_subcommand(parsed_args).await {
        Ok(()) => Ok(()),
        Err(error) => {
            // We are catching the error and pretty printing it instead of letting the
            // program error. Since we are not meant to be used other programs, I think
            // it's fine to always return a 0 error code, but we might wanna revisit this.
            print!("{}", error);
            Ok(())
        }
    }
}

async fn execute_subcommand(args: CommandLineArguments) -> ResultWithDefaultError<()> {
    let command = args.cmd;
    let get_default_api_client = || get_api_client(args.proxy.clone());
    let picker = picker::get_picker(args.fzf);
    if let Some(directory) = args.directory {
        if !directory.exists() {
            return Err(Box::new(error::ArgumentError::DirectoryNotFound(directory)));
        }
        if !directory.is_dir() {
            return Err(Box::new(error::ArgumentError::NotADirectory(directory)));
        }
        std::env::set_current_dir(directory)?;
    }
    match command {
        None => RunningTimeEntryCommand::execute(get_default_api_client()?).await?,
        Some(subcommand) => match subcommand {
            Stop => {
                StopCommand::execute(&get_default_api_client()?, StopCommandOrigin::CommandLine)
                    .await?;
            }
            Continue { interactive } => {
                let picker = if interactive { Some(picker) } else { None };
                ContinueCommand::execute(get_default_api_client()?, picker).await?
            }
            List { number } => ListCommand::execute(get_default_api_client()?, number).await?,
            Current | Running => {
                RunningTimeEntryCommand::execute(get_default_api_client()?).await?
            }
            Start {
                interactive,
                billable,
                description,
                project,
            } => {
                StartCommand::execute(
                    get_default_api_client()?,
                    picker,
                    description,
                    project,
                    billable,
                    interactive,
                )
                .await?
            }
            Auth { api_token } => {
                let credentials = Credentials { api_token };
                let api_client = V9ApiClient::from_credentials(credentials, args.proxy)?;
                AuthenticationCommand::execute(io::stdout(), api_client, get_storage()).await?
            }

            Config {
                delete,
                cmd,
                edit,
                path,
            } => match cmd {
                Some(config_command) => match config_command {
                    ConfigSubCommand::Init => {
                        config::init::ConfigInitCommand::execute(edit).await?;
                    }
                    ConfigSubCommand::Active => {
                        config::active::ConfigActiveCommand::execute().await?;
                    }
                },
                None => config::manage::ConfigManageCommand::execute(delete, edit, path).await?,
            },
        },
    }

    Ok(())
}

fn get_api_client(proxy: Option<String>) -> ResultWithDefaultError<impl ApiClient> {
    let credentials_storage = get_storage();
    return match credentials_storage.read() {
        Ok(credentials) => V9ApiClient::from_credentials(credentials, proxy),
        Err(err) => {
            println!(
                "{}\n{} {}",
                "Please set your API token first by calling toggl auth <API_TOKEN>.".red(),
                "You can find your API token at".blue().bold(),
                "https://track.toggl.com/profile".blue().bold().underline()
            );
            Err(err)
        }
    };
}

fn get_storage() -> impl CredentialsStorage {
    let keyring = Entry::new("togglcli", "default")
        .unwrap_or_else(|err| panic!("Couldn't create credentials_storage: {err}"));
    KeyringStorage::new(keyring)
}
