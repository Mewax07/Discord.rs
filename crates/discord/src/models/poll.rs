use serde::{Deserialize, Serialize};

pub const POLL_MIN_HOURS: u32 = 1;
pub const POLL_MAX_HOURS: u32 = 768;
pub const POLL_MAX_ANSWERS: usize = 10;
pub const POLL_QUESTION_LEN: usize = 300;
pub const POLL_ANSWER_LEN: usize = 55;

#[derive(Debug, Clone, Serialize)]
pub struct PollMedia {
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PollAnswerRequest {
    pub poll_media: PollMedia,
}

#[derive(Debug, Clone, Serialize)]
pub struct PollRequest {
    pub question: PollMedia,
    pub answers: Vec<PollAnswerRequest>,
    pub duration: u32,
    pub allow_multiselect: bool,
    pub layout_type: u8,
}

impl PollRequest {
    pub fn new(question: impl Into<String>, answers: Vec<String>, hours: u32) -> Self {
        Self {
            question: PollMedia {
                text: question.into(),
            },
            answers: answers
                .into_iter()
                .map(|text| PollAnswerRequest {
                    poll_media: PollMedia { text },
                })
                .collect(),
            duration: hours.clamp(POLL_MIN_HOURS, POLL_MAX_HOURS),
            allow_multiselect: false,
            layout_type: 1,
        }
    }

    pub fn multiselect(mut self, allowed: bool) -> Self {
        self.allow_multiselect = allowed;
        self
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PollMediaData {
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PollAnswer {
    #[serde(default)]
    pub answer_id: u32,
    #[serde(default)]
    pub poll_media: PollMediaData,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PollAnswerCount {
    #[serde(default)]
    pub id: u32,
    #[serde(default)]
    pub count: u32,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PollResults {
    #[serde(default)]
    pub is_finalized: bool,
    #[serde(default)]
    pub answer_counts: Vec<PollAnswerCount>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Poll {
    #[serde(default)]
    pub question: PollMediaData,
    #[serde(default)]
    pub answers: Vec<PollAnswer>,
    #[serde(default)]
    pub results: Option<PollResults>,
    #[serde(default)]
    pub expiry: Option<String>,
    #[serde(default)]
    pub allow_multiselect: bool,
}

impl Poll {
    pub fn tally(&self) -> Vec<(String, u32)> {
        let counts = self.results.clone().unwrap_or_default();

        self.answers
            .iter()
            .map(|answer| {
                let label = answer
                    .poll_media
                    .text
                    .clone()
                    .unwrap_or_else(|| format!("Answer {}", answer.answer_id));
                let votes = counts
                    .answer_counts
                    .iter()
                    .find(|entry| entry.id == answer.answer_id)
                    .map(|entry| entry.count)
                    .unwrap_or(0);
                (label, votes)
            })
            .collect()
    }

    pub fn total_votes(&self) -> u32 {
        self.results
            .as_ref()
            .map(|results| results.answer_counts.iter().map(|c| c.count).sum())
            .unwrap_or(0)
    }

    pub fn is_finalized(&self) -> bool {
        self.results
            .as_ref()
            .map(|results| results.is_finalized)
            .unwrap_or(false)
    }
}
