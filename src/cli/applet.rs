use anyhow::Error as Anyhow;
use async_trait::async_trait;
use clap::Subcommand;
use derive_more::From;

mod analyze;
mod play;
mod uci;

/// Trait for types that behave like subcommands.
#[async_trait]
pub trait Execute {
    /// Execute the subcommand.
    async fn execute(self) -> Result<(), Anyhow>;
}

#[derive(From, Subcommand)]
pub enum Applet {
    Analyze(analyze::Analyze),
    Play(play::Play),
    Uci(uci::Uci),
}

impl Default for Applet {
    fn default() -> Self {
        uci::Uci::default().into()
    }
}

#[async_trait]
impl Execute for Applet {
    async fn execute(self) -> Result<(), Anyhow> {
        match self {
            Applet::Analyze(a) => Ok(a.execute().await?),
            Applet::Play(a) => Ok(a.execute().await?),
            Applet::Uci(a) => Ok(a.execute().await?),
        }
    }
}
