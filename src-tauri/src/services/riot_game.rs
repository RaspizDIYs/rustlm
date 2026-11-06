use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RiotGame {
    #[default]
    League,
    Tft,
    Valorant,
}
impl RiotGame {
    pub fn name(self) -> &'static str {
        match self {
            Self::League => "League of Legends",
            Self::Tft => "TFT",
            Self::Valorant => "Valorant",
        }
    }
    pub fn product(self) -> &'static str {
        match self {
            Self::Valorant => "valorant",
            _ => "league_of_legends",
        }
    }
    pub fn uses_league(self) -> bool {
        self != Self::Valorant
    }
    pub fn launch_args(self) -> [String; 2] {
        [
            format!("--launch-product={}", self.product()),
            "--launch-patchline=live".into(),
        ]
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn riot_account_is_shared_but_product_and_league_lifecycle_are_separate() {
        assert_eq!(RiotGame::Tft.product(), RiotGame::League.product());
        assert_eq!(
            RiotGame::Valorant.launch_args()[0],
            "--launch-product=valorant"
        );
        assert!(!RiotGame::Valorant.uses_league());
        assert!(serde_json::from_str::<RiotGame>("\"--arbitrary-argument\"").is_err());
    }
}
