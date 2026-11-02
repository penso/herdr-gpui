use super::*;
use herdr_chat_protocol::Tokens;

fn plain(text: String) -> String {
    text.replace(['\u{2068}', '\u{2069}'], "")
}

/// English reads as the protocol's own `display` writes it.
#[test]
fn english_matches_the_protocol() {
    for ms in [950, 12_000, 184_000, 3_720_000] {
        assert_eq!(plain(duration(ms)), display::duration(ms));
    }
    let ends = [
        TurnEnd::Done,
        TurnEnd::Cancelled,
        TurnEnd::Limit,
        TurnEnd::Refused,
        TurnEnd::Failed,
        TurnEnd::Other,
    ];
    let tokens = [
        None,
        Some(Tokens {
            input: 3_200,
            output: 450,
            ..Tokens::default()
        }),
        Some(Tokens {
            input: 18_600,
            output: 1_200_000,
            thought: 90,
            ..Tokens::default()
        }),
    ];
    for end in ends {
        for tokens in &tokens {
            let summary = TurnSummary {
                duration_ms: 12_000,
                ended_ms: None,
                end,
                tokens: *tokens,
            };
            assert_eq!(plain(turn(&summary)), summary.describe());
        }
    }
}
