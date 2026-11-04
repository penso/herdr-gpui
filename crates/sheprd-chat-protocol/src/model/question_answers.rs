//! Keep historical question cards bounded before they reach a client view.

use serde::{Deserializer, Serialize, Serializer, de, ser};

use super::QuestionAnswer;

pub const MAX_QUESTION_ANSWERS: usize = 32;
/// Aggregate UTF-8 bytes of the questions and answers in one notice.
pub const MAX_QUESTION_TEXT: usize = 128 * 1024;

pub fn valid_question_answers(answers: &[QuestionAnswer]) -> bool {
    let mut bytes = 0usize;
    answers.len() <= MAX_QUESTION_ANSWERS
        && answers.iter().all(|answer| {
            bytes = bytes
                .saturating_add(answer.question.len())
                .saturating_add(answer.answer.len());
            bytes <= MAX_QUESTION_TEXT
        })
}

pub(super) fn serialize<S: Serializer>(
    answers: &[QuestionAnswer],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    if !valid_question_answers(answers) {
        return Err(ser::Error::custom(
            "question answers exceed their count or text limit",
        ));
    }
    answers.serialize(serializer)
}

pub(super) fn deserialize<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<QuestionAnswer>, D::Error> {
    struct Answers;
    impl<'de> de::Visitor<'de> for Answers {
        type Value = Vec<QuestionAnswer>;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a bounded list of historical question answers")
        }

        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut answers = Vec::new();
            let mut bytes = 0usize;
            while let Some(answer) = seq.next_element::<QuestionAnswer>()? {
                bytes = bytes
                    .saturating_add(answer.question.len())
                    .saturating_add(answer.answer.len());
                if answers.len() == MAX_QUESTION_ANSWERS || bytes > MAX_QUESTION_TEXT {
                    return Err(de::Error::custom(
                        "question answers exceed their count or text limit",
                    ));
                }
                answers.push(answer);
            }
            Ok(answers)
        }
    }
    deserializer.deserialize_seq(Answers)
}
