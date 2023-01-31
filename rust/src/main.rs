use std::str::FromStr;
use tracing::info;

fn main() -> anyhow::Result<()> {
    let args: artifact_executor::args::Args = argh::from_env();

    let trace_level = tracing::Level::from_str(&args.log_level)
        .map_err(anyhow::Error::from)
        .map_err(|err| {
            err.context(format!(
                "provided log level, \"{}\", is not a supported tracing::Level",
                args.log_level
            ))
        })?;
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(trace_level)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .map_err(anyhow::Error::from)
        .map_err(|err| err.context("unable to set tracing subscriber global default"))?;

    info!("Arguments parsed and logging initialized");

    let working_directory = std::env::current_dir()
        .map_err(anyhow::Error::from)
        .map_err(|err| err.context("failed to determine current working directory"))?;
    info!("Working directory: {:?}", working_directory);

    match args.command {
        artifact_executor::args::Command::Execute(command) => {
            let _execute = artifact_executor::execute::Execute::from_command(command)?;
        }
    };

    Ok(())
}
