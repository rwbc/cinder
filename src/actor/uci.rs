use crate::*;
use anyhow::{anyhow, Context, Error as Failure};
use async_trait::async_trait;
use derivative::Derivative;
use smol::block_on;
use std::error::Error;
use tracing::*;
use vampirc_uci::{parse_one, UciFen, UciMessage};

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Uci<R>
where
    R: Remote,
    R::Error: Error + Send + Sync + 'static,
{
    #[derivative(Debug = "ignore")]
    remote: R,
}

impl<R> Uci<R>
where
    R: Remote,
    R::Error: Error + Send + Sync + 'static,
{
    #[instrument(skip(remote), err)]
    pub async fn init(mut remote: R) -> Result<Self, R::Error> {
        remote.send(UciMessage::Uci).await?;

        loop {
            debug!("expecting 'uciok'");
            match parse_one(&remote.recv().await?) {
                UciMessage::UciOk => break,
                UciMessage::Id { name, author } => {
                    if let Some(engine) = name {
                        info!(?engine)
                    }

                    if let Some(author) = author {
                        info!(?author)
                    }
                }
                m => Self::ignore(m),
            }
        }

        Ok(Uci { remote })
    }

    fn ignore(msg: UciMessage) {
        let e = match msg {
            UciMessage::Unknown(m, cause) => {
                let error = anyhow!("ignoring invalid UCI command '{}'", m);
                match cause {
                    Some(cause) => Failure::from(cause).context(error),
                    None => error,
                }
            }

            msg => anyhow!("ignoring unexpected UCI command '{}'", msg),
        };

        warn!("{:?}", e);
    }
}

impl<R> Drop for Uci<R>
where
    R: Remote,
    R::Error: Error + Send + Sync + 'static,
{
    #[instrument(skip(self))]
    fn drop(&mut self) {
        if let Err(e) = block_on(self.remote.send(UciMessage::Stop))
            .context("failed to gracefully stop the uci engine")
        {
            error!("{:?}", e);
        }

        if let Err(e) = block_on(self.remote.send(UciMessage::Quit))
            .context("failed to gracefully quit the uci engine")
        {
            error!("{:?}", e);
        }
    }
}

#[async_trait]
impl<R> Actor for Uci<R>
where
    R: Remote + Send + Sync,
    R::Error: Error + Send + Sync + 'static,
{
    type Error = R::Error;

    #[instrument(skip(self, p), err)]
    async fn act(&mut self, p: Position) -> Result<PlayerAction, Self::Error> {
        let setpos = UciMessage::Position {
            startpos: false,
            fen: Some(UciFen(p.to_string())),
            moves: Vec::new(),
        };

        let go = UciMessage::Go {
            time_control: None,
            search_control: None,
        };

        self.remote.send(setpos).await?;
        self.remote.send(go).await?;

        let m = loop {
            debug!("expecting 'bestmove'");
            match parse_one(&self.remote.recv().await?) {
                UciMessage::BestMove { best_move: m, .. } => break m.into(),
                i @ UciMessage::Info(_) => debug!("{}", i),
                m => Self::ignore(m),
            }
        };

        Ok(PlayerAction::MakeMove(m))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MockRemote;
    use mockall::{predicate::*, Sequence};
    use proptest::prelude::*;
    use smol::block_on;
    use std::io;

    fn unexpected_uci_command() -> impl Strategy<Value = UciMessage> {
        prop_oneof![
            Just(UciMessage::UciOk),
            Just(UciMessage::UciNewGame),
            Just(UciMessage::ReadyOk),
            Just(UciMessage::Stop),
            Just(UciMessage::Quit),
            Just(UciMessage::PonderHit),
            any::<bool>().prop_map(UciMessage::Debug),
            any::<(Option<String>, Option<String>)>()
                .prop_map(|(name, author)| UciMessage::Id { name, author }),
            any::<(bool, Option<String>, Option<String>)>()
                .prop_map(|(later, name, code)| UciMessage::Register { later, name, code }),
            any::<(String, Option<String>)>()
                .prop_map(|(name, value)| UciMessage::SetOption { name, value }),
        ]
    }

    proptest! {
        #[test]
        fn init_shakes_hand_with_engine(name: String, author: String) {
            let mut remote = MockRemote::new();

            remote.expect_send().times(1)
                .with(eq(UciMessage::Uci))
                .return_once(|_| Ok(()));

            remote.expect_recv().times(1)
                .return_once(move || Ok(UciMessage::id_name(&name).to_string()));

            remote.expect_recv().times(1)
                .return_once(move || Ok(UciMessage::id_author(&author).to_string()));

            remote.expect_recv().times(1)
                .return_once(move || Ok(UciMessage::UciOk.to_string()));

            remote.expect_send().times(1)
                .with(eq(UciMessage::Stop))
                .return_once(|_| Ok(()));

            remote.expect_send().times(1)
                .with(eq(UciMessage::Quit))
                .return_once(|_| Ok(()));

            assert!(block_on(Uci::init(remote)).is_ok());
        }

        #[test]
        fn init_can_fail(e: io::Error) {
            let mut remote = MockRemote::new();

            remote.expect_send().times(1)
                .with(eq(UciMessage::Uci))
                .return_once(|_| Ok(()));

            let kind = e.kind();
            remote.expect_recv().times(1)
                .return_once(move || Err(e));

            assert_eq!(block_on(Uci::init(remote)).unwrap_err().kind(), kind);
        }

        #[test]
        fn engine_can_make_a_move(pos: Position, m: Move) {
            let mut remote = MockRemote::new();
            let mut seq = Sequence::new();

            let fen = pos.to_string();
            remote.expect_send().times(1).in_sequence(&mut seq)
                .with(function(move |msg: &UciMessage| msg.to_string().contains(&fen)))
                .return_once(|_| Ok(()));

            remote.expect_send().times(1).in_sequence(&mut seq)
                .with(function(move |msg: &UciMessage| msg.to_string().starts_with("go")))
                .return_once(|_| Ok(()));

            remote.expect_recv().times(1)
                .return_once(move || Ok(format!("bestmove {}", m)));

            remote.expect_send().times(1)
                .with(eq(UciMessage::Stop))
                .return_once(|_| Ok(()));

            remote.expect_send().times(1)
                .with(eq(UciMessage::Quit))
                .return_once(|_| Ok(()));

            let mut uci = Uci { remote };
            assert_eq!(block_on(uci.act(pos)).unwrap(), PlayerAction::MakeMove(m));
        }

        #[test]
        fn act_ignores_invalid_uci_commands(pos: Position, m: Move, cmd in "[^bestmove]+") {
            let mut remote = MockRemote::new();
            let mut seq = Sequence::new();

            let fen = pos.to_string();
            remote.expect_send().times(1).in_sequence(&mut seq)
                .with(function(move |msg: &UciMessage| msg.to_string().contains(&fen)))
                .return_once(|_| Ok(()));

            remote.expect_send().times(1).in_sequence(&mut seq)
                .with(function(move |msg: &UciMessage| msg.to_string().starts_with("go")))
                .return_once(|_| Ok(()));

            let mut cmd = Some(cmd);
            remote.expect_recv().times(2)
                .returning(move || Ok(cmd.take().unwrap_or_else(|| format!("bestmove {}", m))));

            remote.expect_send().times(1)
                .with(eq(UciMessage::Stop))
                .return_once(|_| Ok(()));

            remote.expect_send().times(1)
                .with(eq(UciMessage::Quit))
                .return_once(|_| Ok(()));

            let mut uci = Uci { remote };
            assert_eq!(block_on(uci.act(pos)).unwrap(), PlayerAction::MakeMove(m));
        }

        #[test]
        fn act_ingnores_unexpected_uci_commands(pos: Position, m: Move, cmd in unexpected_uci_command()) {
            let mut remote = MockRemote::new();
            let mut seq = Sequence::new();

            let fen = pos.to_string();
            remote.expect_send().times(1).in_sequence(&mut seq)
                .with(function(move |msg: &UciMessage| msg.to_string().contains(&fen)))
                .return_once(|_| Ok(()));

            remote.expect_send().times(1).in_sequence(&mut seq)
                .with(function(move |msg: &UciMessage| msg.to_string().starts_with("go")))
                .return_once(|_| Ok(()));

            let mut cmd = Some(cmd.to_string());
            remote.expect_recv().times(2)
                .returning(move || Ok(cmd.take().unwrap_or_else(|| format!("bestmove {}", m))));

            remote.expect_send().times(1)
                .with(eq(UciMessage::Stop))
                .return_once(|_| Ok(()));

            remote.expect_send().times(1)
                .with(eq(UciMessage::Quit))
                .return_once(|_| Ok(()));

            let mut uci = Uci { remote };
            assert_eq!(block_on(uci.act(pos)).unwrap(), PlayerAction::MakeMove(m));
        }

        #[test]
        fn act_can_fail_writing_to_remote(pos: Position, e: io::Error) {
            let mut remote = MockRemote::new();

            let kind = e.kind();
            remote.expect_send().times(1)
                .return_once(move |_: UciMessage| Err(e));

            remote.expect_send().times(1)
                .with(eq(UciMessage::Stop))
                .return_once(|_| Ok(()));

            remote.expect_send().times(1)
                .with(eq(UciMessage::Quit))
                .return_once(|_| Ok(()));

            let mut uci = Uci { remote };
            assert_eq!(block_on(uci.act(pos)).unwrap_err().kind(), kind);
        }

        #[test]
        fn act_can_fail_reading_from_remote(pos: Position, e: io::Error) {
            let mut remote = MockRemote::new();

            remote.expect_send().times(2)
                .returning(|_: UciMessage| Ok(()));

            let kind = e.kind();
            remote.expect_recv().return_once(move || Err(e));

            remote.expect_send().times(1)
                .with(eq(UciMessage::Stop))
                .return_once(|_| Ok(()));

            remote.expect_send().times(1)
                .with(eq(UciMessage::Quit))
                .return_once(|_| Ok(()));

            let mut uci = Uci { remote };
            assert_eq!(block_on(uci.act(pos)).unwrap_err().kind(), kind);
        }
    }
}
