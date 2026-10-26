//! Async question replies need their exact earlier tool request as provenance.
//! Native reply records may carry only `user.text`, which is not sufficient.

use std::collections::HashMap;

use herdr_chat_protocol::{
    EntryKind, MAX_QUESTION_ANSWERS, QuestionAnswer, ToolContent, valid_question_answers,
};
use serde::Deserialize;
use serde_json::Value;

use super::{MAX_ENTRIES, MAX_ID, MAX_TRANSCRIPT_TEXT, Projection, TEXT, notices, string};

const TOOL: &str = "request_user_input_async";

/// One question of an async request: the tool call that asked it and its
/// position among that call's questions.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct QuestionKey {
    pub(super) call: String,
    pub(super) index: usize,
}

#[derive(Deserialize)]
struct Reply {
    answer: String,
    question: String,
    #[serde(rename = "questionItemId")]
    question_item_id: String,
}

impl Projection {
    /// Native retained answers already establish provenance, including when
    /// their original tool request has fallen outside the bounded read window.
    pub(super) fn retained_answer(&mut self, value: &Value, order: &str) -> Option<EntryKind> {
        let source = value.to_string();
        self.transcript.truncated |= source.len() > TEXT;
        let fallback = || notices::source("Retained question context".into(), &source);
        let Some((call, rows)) = retained_rows(value, order) else {
            return Some(fallback());
        };
        let pending = rows
            .into_iter()
            .enumerate()
            .map(|(index, answer)| {
                let call = call.to_owned();
                (QuestionKey { call, index }, answer)
            })
            .filter(|(key, answer)| self.answered.get(key) != Some(answer))
            .collect::<Vec<_>>();
        if pending.is_empty() {
            return None;
        }
        if !self.remember_answers(&pending) {
            return Some(fallback());
        }
        let answers = pending.into_iter().map(|(_, answer)| answer).collect();
        Some(notices::answered(answers, &source))
    }

    pub(super) fn question_reply(&mut self, record: &Value, content: &str) -> Option<EntryKind> {
        if content.len() > TEXT || record["metadata"]["client_authored"].as_bool() != Some(false) {
            return None;
        }
        let body = content
            .trim()
            .strip_prefix("<send_user_message_question_reply>")?
            .strip_suffix("</send_user_message_question_reply>")?;
        let replies: Vec<Reply> = serde_json::from_str(body).ok()?;
        if replies.is_empty() || replies.len() > MAX_QUESTION_ANSWERS {
            return None;
        }
        let mut answered = HashMap::new();
        let mut pending = Vec::with_capacity(replies.len());
        for reply in replies {
            let (name, call, index): (String, String, usize) =
                serde_json::from_str(&reply.question_item_id).ok()?;
            if name != TOOL || reply.question.trim().is_empty() {
                return None;
            }
            let key = QuestionKey { call, index };
            if answered.contains_key(&key) {
                return None;
            }
            let retained = self
                .answered
                .get(&key)
                .is_some_and(|answer| answer.question == reply.question);
            if !retained {
                let position = *self.tools.get(&key.call)?;
                let EntryKind::Tool(tool) = &self.transcript.entries.get(position)?.kind else {
                    return None;
                };
                if tool.title != TOOL {
                    return None;
                }
                let ToolContent::Text { text } = tool.content.first()? else {
                    return None;
                };
                let arguments: Value = serde_json::from_str(text).ok()?;
                let question = arguments["questions"].as_array()?.get(index)?;
                if string(question, "title") != reply.question {
                    return None;
                }
            }
            let answer = QuestionAnswer {
                question: reply.question,
                answer: reply.answer,
            };
            answered.insert(key.clone(), answer.clone());
            if self.answered.get(&key) != Some(&answer) {
                pending.push((key, answer));
            }
        }
        let answers = answered.into_values().collect::<Vec<_>>();
        if !valid_question_answers(&answers) || !self.remember_answers(&pending) {
            return None;
        }
        // A malformed batch must not consume earlier valid answers from it.
        if pending.is_empty() {
            return Some(notices::source(
                "Question answer already recorded".into(),
                content,
            ));
        }
        let answers = pending.into_iter().map(|(_, answer)| answer).collect();
        Some(notices::answered(answers, content))
    }

    fn remember_answers(&mut self, pending: &[(QuestionKey, QuestionAnswer)]) -> bool {
        let size = |key: &QuestionKey, answer: &QuestionAnswer| {
            key.call.len() + answer.question.len() + answer.answer.len()
        };
        let mut bytes: usize = self
            .answered
            .iter()
            .map(|(key, answer)| size(key, answer))
            .sum();
        let mut count = self.answered.len();
        for (key, answer) in pending {
            if let Some(previous) = self.answered.get(key) {
                bytes -= size(key, previous);
            } else {
                count += 1;
            }
            bytes += size(key, answer);
        }
        if count > MAX_ENTRIES || bytes > MAX_TRANSCRIPT_TEXT {
            return false;
        }
        self.answered.extend(pending.iter().cloned());
        true
    }
}

fn retained_rows<'a>(value: &'a Value, order: &str) -> Option<(&'a str, Vec<QuestionAnswer>)> {
    let call = value["call_id"].as_str()?;
    if call.is_empty()
        || call.len() > MAX_ID
        || string(value, "turn_id").is_empty()
        || value[order].as_u64().is_none()
    {
        return None;
    }
    let rows = value["questions"].as_array()?;
    if rows.is_empty() || rows.len() > MAX_QUESTION_ANSWERS {
        return None;
    }
    let answers = Vec::<QuestionAnswer>::deserialize(&value["questions"]).ok()?;
    (valid_question_answers(&answers)
        && answers
            .iter()
            .all(|answer| !answer.question.trim().is_empty()))
    .then_some((call, answers))
}
