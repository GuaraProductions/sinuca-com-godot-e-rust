use crate::ball::BallType;
use std::fmt;

#[derive(Default)]
pub struct Player {
    pub ball_numeration: BallType,
    id: u8,
    points: u8,
    name: String,
    is_bot: bool,
}

impl Player {
    pub fn new(id: u8, ball_numeration: BallType) -> Self {
        Self {
            id,
            ball_numeration,
            points: 0,
            name: format!("Jogador {}", id),
            is_bot: false,
        }
    }

    /// Blank names fall back to `default`.
    pub fn with_name(mut self, name: &str, default: &str) -> Self {
        let trimmed = name.trim();
        self.name = if trimmed.is_empty() { default.to_string() } else { trimmed.to_string() };
        self
    }

    pub fn with_bot(mut self, is_bot: bool) -> Self {
        self.is_bot = is_bot;
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_bot(&self) -> bool {
        self.is_bot
    }

    pub fn add_points(&mut self, points: u8) {
        self.points += points;
    }

    pub fn has_enumeration(&self) -> bool {
        self.ball_numeration != BallType::Unidentified
    }

    pub fn is_same_enumeration(&self, numeration: BallType) -> bool {
        self.ball_numeration == numeration
    }

    pub fn id(&self) -> u8 {
        self.id
    }

    pub fn points(&self) -> u8 {
        self.points
    }

    pub fn set_enumeration(&mut self, numeration: BallType) {
        self.ball_numeration = numeration;
    }
}

impl PartialEq<Player> for Player {
    fn eq(&self, other: &Player) -> bool {
        self.id == other.id
    }
}

impl fmt::Display for Player {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "id {}, ball_numeration {}, points {}",
            self.id, self.ball_numeration, self.points
        )
    }
}
