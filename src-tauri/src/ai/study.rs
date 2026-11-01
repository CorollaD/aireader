//! Study material from a book: flashcards to keep, and a quiz to take now. The model replies in
//! JSON; a reply that can't be used is sent back once with the reason.

use anyhow::{Result, anyhow};
use rig_core::completion::CompletionRequest;
use rig_core::message::Message;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::tasks::{Runner, SummaryCache, Unit};
use crate::model::Document;

const MAX_CARDS: usize = 20;
const QUIZ_QUESTIONS: usize = 5;
const STUDY_TOKENS: u64 = 3_000;

/// The text to study.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StudyScope {
    Chapter {
        chapter: usize,
    },
    Pages {
        from: usize,
        to: usize,
    },
    /// Given text, e.g. the reader's highlights.
    Text {
        title: String,
        text: String,
    },
}

/// A suggested flashcard.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StudyCard {
    pub front: String,
    pub back: String,
    /// A short excerpt it is based on, word for word.
    #[serde(default)]
    pub quote: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QuizQuestion {
    pub question: String,
    pub options: Vec<String>,
    /// Index of the right option.
    pub answer: usize,
    pub explanation: String,
    #[serde(default)]
    pub quote: String,
}

#[derive(Deserialize)]
struct CardList {
    cards: Vec<StudyCard>,
}

#[derive(Deserialize)]
struct Quiz {
    questions: Vec<QuizQuestion>,
}

/// (What it is about, the text to send.)
pub async fn material(
    r: &Runner<'_>,
    cache: &mut SummaryCache,
    doc: &Document,
    book: &str,
    scope: StudyScope,
) -> Result<(String, String)> {
    use super::tasks::{chapter_unit, pages_unit, split_by_budget};
    let unit: Unit = match scope {
        StudyScope::Text { title, text } => {
            let text = split_by_budget(&text, r.budget)
                .into_iter()
                .next()
                .unwrap_or_default();
            return Ok((title, text));
        }
        StudyScope::Chapter { chapter } => chapter_unit(doc, chapter)?,
        StudyScope::Pages { from, to } => pages_unit(doc, from, to)?,
    };
    let text = super::tasks::unit_material(r, cache, book, &unit).await?;
    Ok((unit.title, text))
}

/// The JSON object in a reply, which may be wrapped in a code fence or a sentence.
fn parse_json<T: DeserializeOwned>(reply: &str) -> Result<T> {
    let start = reply
        .find('{')
        .ok_or_else(|| anyhow!("no JSON in the reply"))?;
    let end = reply
        .rfind('}')
        .ok_or_else(|| anyhow!("no JSON in the reply"))?;
    serde_json::from_str(&reply[start..=end]).map_err(|e| anyhow!("invalid JSON: {e}"))
}

/// Sends a request for JSON; `read` turns the reply into the result, or says what is wrong.
async fn ask_json<T>(
    r: &Runner<'_>,
    prompt: String,
    read: impl Fn(&str) -> Result<T>,
) -> Result<T> {
    let reply = r.complete(prompt.clone(), STUDY_TOKENS).await?;
    let problem = match read(&reply) {
        Ok(value) => return Ok(value),
        Err(e) => e,
    };
    let request = CompletionRequest::new(Message::user(format!(
        "Your answer could not be used ({problem}). Reply with only the required JSON."
    )))
    .messages(vec![Message::user(prompt), Message::assistant(reply)])
    .preamble(r.system())
    .max_tokens(STUDY_TOKENS);
    read(&r.complete_request(request).await?)
}

fn read_cards(reply: &str) -> Result<Vec<StudyCard>> {
    let list: CardList = parse_json(reply)?;
    let mut seen = std::collections::HashSet::new();
    let cards: Vec<StudyCard> = list
        .cards
        .into_iter()
        .map(|c| StudyCard {
            front: c.front.trim().to_owned(),
            back: c.back.trim().to_owned(),
            quote: c.quote.trim().to_owned(),
        })
        .filter(|c| !c.front.is_empty() && !c.back.is_empty() && seen.insert(c.front.clone()))
        .take(MAX_CARDS)
        .collect();
    if cards.is_empty() {
        anyhow::bail!("there are no usable cards");
    }
    Ok(cards)
}

fn read_quiz(reply: &str) -> Result<Vec<QuizQuestion>> {
    let quiz: Quiz = parse_json(reply)?;
    let questions: Vec<QuizQuestion> = quiz
        .questions
        .into_iter()
        .filter(|q| {
            !q.question.trim().is_empty()
                && (2..=6).contains(&q.options.len())
                && q.answer < q.options.len()
                && q.options.iter().all(|o| !o.trim().is_empty())
        })
        .take(QUIZ_QUESTIONS)
        .collect();
    if questions.is_empty() {
        anyhow::bail!("there are no usable questions");
    }
    Ok(questions)
}

/// Flashcards on what is worth remembering in `material`.
pub async fn cards(
    r: &Runner<'_>,
    book: &str,
    topic: &str,
    material: &str,
    count: usize,
) -> Result<Vec<StudyCard>> {
    let count = count.clamp(1, MAX_CARDS);
    let prompt = format!(
        "Below is \"{topic}\" from the book \"{book}\". Write up to {count} flashcards for what \
         is most worth remembering: key ideas, facts, terms, and how they connect. Skip trivia. \
         Each question must make sense on its own, without the book at hand, and each answer \
         must be short.\n\n\
         Reply with only this JSON: {{\"cards\": [{{\"front\": \"question\", \"back\": \
         \"answer\", \"quote\": \"up to 20 words copied exactly from the text below\"}}]}}\n\n\
         <content>\n{material}\n</content>"
    );
    ask_json(r, prompt, read_cards).await
}

/// A multiple-choice quiz on `material`.
pub async fn quiz(
    r: &Runner<'_>,
    book: &str,
    topic: &str,
    material: &str,
) -> Result<Vec<QuizQuestion>> {
    let prompt = format!(
        "Below is \"{topic}\" from the book \"{book}\". Write a quiz of {QUIZ_QUESTIONS} \
         multiple-choice questions that test understanding, not memory of small details. Give \
         each question four options, one of them right, with believable wrong ones; vary which \
         position is right.\n\n\
         Reply with only this JSON: {{\"questions\": [{{\"question\": \"...\", \"options\": \
         [\"...\", \"...\", \"...\", \"...\"], \"answer\": 0, \"explanation\": \"why the answer \
         is right, in one or two sentences\", \"quote\": \"up to 20 words copied exactly from \
         the text below\"}}]}} where answer is the index of the right option.\n\n\
         <content>\n{material}\n</content>"
    );
    ask_json(r, prompt, read_quiz).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cards_are_cleaned_and_deduplicated() {
        let reply = "Here you go:\n```json\n{\"cards\": [
            {\"front\": \" 什么是赤壁之战？ \", \"back\": \"一场水战\", \"quote\": \"赤壁\"},
            {\"front\": \"什么是赤壁之战？\", \"back\": \"重复\"},
            {\"front\": \"\", \"back\": \"没有问题\"}
        ]}\n```";
        let cards = read_cards(reply).unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].front, "什么是赤壁之战？");
        assert!(read_cards("{\"cards\": []}").is_err());
        assert!(read_cards("no json").is_err());
    }

    #[test]
    fn quiz_questions_must_have_a_valid_answer() {
        let reply = r#"{"questions": [
            {"question": "Q1", "options": ["a", "b", "c", "d"], "answer": 2, "explanation": "e"},
            {"question": "Q2", "options": ["a", "b"], "answer": 5, "explanation": "e"},
            {"question": "Q3", "options": ["only one"], "answer": 0, "explanation": "e"}
        ]}"#;
        let quiz = read_quiz(reply).unwrap();
        assert_eq!(quiz.len(), 1);
        assert_eq!(quiz[0].answer, 2);
    }

    #[tokio::test]
    async fn an_unusable_reply_is_sent_back_once() {
        use rig_core::test_utils::{MockCompletionModel, MockTurn};
        let model = MockCompletionModel::from_turns([
            MockTurn::text("I'd rather chat."),
            MockTurn::text(r#"{"cards": [{"front": "Q", "back": "A"}]}"#),
        ]);
        let r = Runner {
            model: model.clone().erase(),
            cfg: Default::default(),
            budget: 1_000,
            emit: &|_| {},
        };
        let cards = cards(&r, "书", "第一章", "正文", 5).await.unwrap();
        assert_eq!(cards[0].back, "A");
        assert_eq!(model.requests().len(), 2);
    }
}
