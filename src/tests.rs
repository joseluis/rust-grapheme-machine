use super::{ClusterAction, GraphemeMachine};

// The tests in this file are only for the public-facing `GraphemeCluster`
// API. The internal state machine implementation has its own tests under
// `crate::state::tests`, where most of the interesting testing happens.

use pretty_assertions::assert_eq;
#[rustfmt::skip]
use ::u8char::AsU8Chars;

#[test]
fn basics() {
    let mut clusters: Vec<String> = Vec::new();
    let mut current_cluster = String::new();
    let mut machine = GraphemeMachine::new();
    let input = "Hello!\r\nBeep 🧑‍🌾";

    for c in input.u8chars() {
        if machine.next_u8char(c) == ClusterAction::Split {
            if !current_cluster.is_empty() {
                clusters.push(current_cluster.clone());
                current_cluster.clear();
            }
        }
        current_cluster.push_str(c.as_str());
    }
    if !current_cluster.is_empty() {
        clusters.push(current_cluster.clone());
    }

    assert_eq!(
        clusters,
        &[
            "H",
            "e",
            "l",
            "l",
            "o",
            "!",
            "\r\n",
            "B",
            "e",
            "e",
            "p",
            " ",
            "🧑‍🌾"
        ]
    );
}

#[test]
fn end_of_input() {
    let mut machine = GraphemeMachine::new();
    let input = "Hello!\r\nBeep 🧑‍🌾";

    for c in input.u8chars() {
        machine.end_of_input(); // effectively forces a cluster boundary
        if machine.next_u8char(c) != ClusterAction::Split {
            panic!("non-split after end_of_input came before {c:?}");
        }
    }
}

#[test]
fn next_chars_from_str() {
    use ClusterAction::*;
    let mut machine = GraphemeMachine::new();
    let input = "Hello!\r\nBeep 🧑‍🌾";
    let got: Vec<_> = machine.next_chars_from_str(input).collect();
    assert_eq!(
        got,
        &[
            (Split, 'H'),
            (Split, 'e'),
            (Split, 'l'),
            (Split, 'l'),
            (Split, 'o'),
            (Split, '!'),
            (Split, '\r'),
            (Continue, '\n'),
            (Split, 'B'),
            (Split, 'e'),
            (Split, 'e'),
            (Split, 'p'),
            (Split, ' '),
            (Split, '🧑'),
            (Continue, '\u{200D}'), // zero-width joiner
            (Continue, '🌾'),
        ]
    );
}
#[test]
const fn const_iter() {
    const INPUT: &str = "Hello!\r\nBeep 🧑‍🌾";
    const CHAR: char = const {
        let mut machine = GraphemeMachine::new();
        let mut iter = machine.next_chars_from_str(INPUT);

        let mut c = '\0';
        let mut i = 0;
        while i < 9 {
            i += 1;
            if let Some((cluster, character)) = iter.next() {
                if cluster.eq(ClusterAction::Split) {
                    c = character;
                }
            }
        }
        c
    };
    assert![CHAR == 'B'];
}
