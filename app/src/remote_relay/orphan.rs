// SPDX-License-Identifier: AGPL-3.0-only
//! Pure, bounded cleanup policy for a possibly orphaned relay process.

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Observation {
    Gone,
    Identity(String),
    Unknown,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CleanupOutcome {
    Gone,
    Replaced,
    StillRunning,
    Unknown,
    NotOwned,
}

impl CleanupOutcome {
    pub(crate) fn confirmed(&self) -> bool {
        matches!(self, Self::Gone | Self::Replaced)
    }
}

pub(crate) fn cleanup(
    authorized_context: bool,
    expected_identity: &str,
    mut probe: impl FnMut() -> Observation,
    mut signal: impl FnMut(bool) -> bool,
    mut pause: impl FnMut(),
) -> CleanupOutcome {
    if !authorized_context {
        return CleanupOutcome::NotOwned;
    }
    if expected_identity.is_empty() {
        return CleanupOutcome::Unknown;
    }

    match classify(probe(), expected_identity) {
        outcome @ (CleanupOutcome::Gone | CleanupOutcome::Replaced | CleanupOutcome::Unknown) => {
            outcome
        }
        CleanupOutcome::StillRunning => {
            let _term_ack = signal(false);
            pause();
            match classify(probe(), expected_identity) {
                outcome @ (CleanupOutcome::Gone
                | CleanupOutcome::Replaced
                | CleanupOutcome::Unknown) => outcome,
                CleanupOutcome::StillRunning => {
                    let _kill_ack = signal(true);
                    pause();
                    classify(probe(), expected_identity)
                }
                CleanupOutcome::NotOwned => unreachable!(),
            }
        }
        CleanupOutcome::NotOwned => unreachable!(),
    }
}

fn classify(observation: Observation, expected_identity: &str) -> CleanupOutcome {
    match observation {
        Observation::Gone => CleanupOutcome::Gone,
        Observation::Identity(identity) if identity.is_empty() => CleanupOutcome::Unknown,
        Observation::Identity(identity) if identity == expected_identity => {
            CleanupOutcome::StillRunning
        }
        Observation::Identity(_) => CleanupOutcome::Replaced,
        Observation::Unknown => CleanupOutcome::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::{cleanup, CleanupOutcome, Observation};

    fn run(
        observations: Vec<Observation>,
        signals: &mut Vec<bool>,
        pauses: &mut usize,
    ) -> CleanupOutcome {
        let mut observations = observations.into_iter();
        cleanup(
            true,
            "expected",
            || observations.next().expect("test supplied enough probes"),
            |kill| {
                signals.push(kill);
                true
            },
            || *pauses += 1,
        )
    }

    #[test]
    fn wrong_context_does_not_run_callbacks() {
        let mut probes = 0;
        let mut signals = 0;
        let mut pauses = 0;

        let outcome = cleanup(
            false,
            "expected",
            || {
                probes += 1;
                Observation::Identity("expected".into())
            },
            |_| {
                signals += 1;
                true
            },
            || pauses += 1,
        );

        assert_eq!(outcome, CleanupOutcome::NotOwned);
        assert_eq!((probes, signals, pauses), (0, 0, 0));
    }

    #[test]
    fn gone_or_replaced_without_signals() {
        let mut signals = Vec::new();
        let mut pauses = 0;
        assert_eq!(
            run(vec![Observation::Gone], &mut signals, &mut pauses),
            CleanupOutcome::Gone
        );
        assert_eq!(
            run(
                vec![Observation::Identity("other".into())],
                &mut signals,
                &mut pauses,
            ),
            CleanupOutcome::Replaced
        );
        assert!(signals.is_empty());
        assert_eq!(pauses, 0);
    }

    #[test]
    fn term_success_but_still_running_gets_kill() {
        let mut signals = Vec::new();
        let mut pauses = 0;
        let outcome = run(
            vec![
                Observation::Identity("expected".into()),
                Observation::Identity("expected".into()),
                Observation::Identity("expected".into()),
            ],
            &mut signals,
            &mut pauses,
        );

        assert_eq!(outcome, CleanupOutcome::StillRunning);
        assert_eq!(signals, vec![false, true]);
        assert_eq!(pauses, 2);
    }

    #[test]
    fn unknown_after_term_does_not_get_kill() {
        let mut signals = Vec::new();
        let mut pauses = 0;
        let outcome = run(
            vec![
                Observation::Identity("expected".into()),
                Observation::Unknown,
            ],
            &mut signals,
            &mut pauses,
        );

        assert_eq!(outcome, CleanupOutcome::Unknown);
        assert_eq!(signals, vec![false]);
        assert_eq!(pauses, 1);
    }

    #[test]
    fn failed_term_and_kill_with_same_identity_is_still_running() {
        let observations = [
            Observation::Identity("expected".into()),
            Observation::Identity("expected".into()),
            Observation::Identity("expected".into()),
        ];
        let mut observations = observations.into_iter();
        let mut signals = Vec::new();
        let mut pauses = 0;
        let outcome = cleanup(
            true,
            "expected",
            || observations.next().unwrap(),
            |kill| {
                signals.push(kill);
                false
            },
            || pauses += 1,
        );

        assert_eq!(outcome, CleanupOutcome::StillRunning);
        assert_eq!(signals, vec![false, true]);
        assert_eq!(pauses, 2);
    }

    #[test]
    fn confirmed_exit_after_failed_signal_is_gone() {
        let mut observations =
            [Observation::Identity("expected".into()), Observation::Gone].into_iter();
        let mut signals = Vec::new();
        let mut pauses = 0;
        let outcome = cleanup(
            true,
            "expected",
            || observations.next().unwrap(),
            |_| {
                signals.push(false);
                false
            },
            || pauses += 1,
        );

        assert_eq!(outcome, CleanupOutcome::Gone);
        assert_eq!(signals, vec![false]);
        assert_eq!(pauses, 1);
    }

    #[test]
    fn malformed_identity_is_unknown_without_permission() {
        let mut signals = Vec::new();
        let mut pauses = 0;
        assert_eq!(
            run(
                vec![Observation::Identity(String::new())],
                &mut signals,
                &mut pauses,
            ),
            CleanupOutcome::Unknown
        );
        assert!(signals.is_empty());
        assert_eq!(pauses, 0);

        let outcome = cleanup(
            true,
            "",
            || panic!("an empty expected identity must not be probed"),
            |_| panic!("an empty expected identity must not be signaled"),
            || panic!("an empty expected identity must not pause"),
        );
        assert_eq!(outcome, CleanupOutcome::Unknown);
    }

    #[test]
    fn callbacks_are_bounded() {
        let mut probes = 0;
        let mut signals = 0;
        let mut pauses = 0;
        let outcome = cleanup(
            true,
            "expected",
            || {
                probes += 1;
                Observation::Identity("expected".into())
            },
            |_| {
                signals += 1;
                false
            },
            || pauses += 1,
        );

        assert_eq!(outcome, CleanupOutcome::StillRunning);
        assert_eq!((probes, signals, pauses), (3, 2, 2));
        assert!(!outcome.confirmed());
        assert!(CleanupOutcome::Gone.confirmed());
        assert!(CleanupOutcome::Replaced.confirmed());
    }
}
