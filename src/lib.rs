use std::result::Result::Ok;

use discord_flows::{
    model::Message,
    ProvidedBot, Bot,
};
use flowsnet_platform_sdk::logger;
use serde::Serialize;
use serde_json::json;
use store_flows::{get, set, del};
use rand::{seq::SliceRandom, thread_rng};

static WINNER_MSG: &str = "Haha! I can't believe it! Looks like I completely crushed you, \
                            Don't worry, though, it's all in good fun. Maybe next time you'll stand a \
                            chance against my unbeatable skills. Until then, enjoy the taste of defeat!";
static LOSER_MSG: &str = "Alright, alright, you got me this time! \
                            I'll begrudgingly admit it, my friend you beat me fair and square. \
                            I'll be back for a rematch soon. Consider yourself lucky, my friend!";
static TIE_MSG: &str = "Ha! It's a tie, my friend! I must say, it's quite a \
                            rare occurrence. Shall we go at it again? ";
static HELP_MSG: &str = "There are 3 options for you: \n\
                            1. By saying \"hit\", you can have another card\n\
                            2. By saying \"stand\", you will take no more cards, so I will reveal the result.\n\
                            3. By saying \"status\", you can know what cards in your hand as well as my face-up card. \n\
                            4. By saying \"help\" or something else, you will see this help message.";
static INTRO_MSG: &str = "Let me introduce the rule of this game for you:\n\
                            1. Cards 1~10 is at their face value. Face card (J, Q, K) count as 10 points. The Card \"A\" counts 11 points. \n\
                            2. First I will give each of us two cards and one of mine is face-down;\n\
                            3. After that, you can take another card by saying \"hit\" for many times \
                            until you stop by saying \"stand\". \n\
                            4. If your point is beyond 21 after a hit, you lose immediately\n\
                            5. After you \"stand\", I will reveal the face-down card and take card until my point goes beyond 17. \n\
                            6. If your point is less than 21 and greater than mine, or my point is greater than 21, you are the winner. \n\
                            7. If my point is less than 21 and greater than yours, I win. (You will lose in step 3 if yours is greater than 21.)\n\
                            8. In other situations, it's a tie.";

type Card = String;
enum HitOutcome { BUST, CONTINUE }
enum GameEnding { PlayerWin, DealerWin, Tie }

#[derive(Serialize)]
struct Game{
    pub dealer_cards: Vec<Card>,
    pub player_cards: Vec<Card>,

    card2use: Vec<Card>
}

impl Game {
    fn sum_of(&self, cards: &Vec<Card>) -> u64{
        let mut sum = 0;
        for c in cards {
            let v = match c.as_str() {
                "2"|"3"|"4"|"5"|"6"|"7"|"8"|"9" => c.parse::<u64>().unwrap(),
                "10"|"J"|"Q"|"K" => 10,
                "A" => 11,
                &_ => panic!("Invalid Card Name")
            };
            sum += v;
        }
        sum
    }
    fn pop_one_card(&mut self) -> Result<Card, &'static str> {
        if let Some(s) = self.card2use.first(){
            let next = s.clone();
            self.card2use.remove(0);
            Ok(next)
        }else{
            Err("Insufficient Cards")
        }
    }
    pub fn init_game(&mut self) -> Result<(), &'static str> {
        for _ in 0..2{
            let card = self.pop_one_card()?;
            self.player_cards.push(card);
        }
        let card = self.pop_one_card()?.clone();
        self.dealer_cards.push(card);
        Ok(())
    }

    pub fn new() -> Game {
        let quatar: Vec<String> = vec!["2", "3", "4", "5", "6", "7", "8", "9", "10", "J", "Q", "K", "A"]
                                        .iter().map(|&s|s.to_owned()).collect();
        let mut one_deck = Vec::new();
        for _ in 0..4 {
            one_deck.extend(quatar.clone());
        }
        let mut rng = thread_rng();
        one_deck.shuffle(&mut rng);

        Game { dealer_cards: Vec::new(), player_cards: Vec::new(), card2use: one_deck }
    }

    pub fn hit(&mut self) -> Result<HitOutcome, &'static str>{
        let card = self.pop_one_card()?;
        self.player_cards.push(card);
        if self.sum_of(&self.player_cards) > 21{
            return Ok(HitOutcome::BUST)
        }
        Ok(HitOutcome::CONTINUE)
    }

    pub fn stand(&mut self) -> Result<GameEnding, &'static str>{
        let card = self.pop_one_card()?;
        self.dealer_cards.push(card);
        while self.sum_of(&self.dealer_cards) < 17{
            let icard = self.pop_one_card()?;
            self.dealer_cards.push(icard);
        }

        let dealer_score = self.sum_of(&self.dealer_cards);
        let player_score = self.sum_of(&self.player_cards);
        if player_score > 21 {
            Ok(GameEnding::DealerWin)
        }else if player_score < dealer_score && dealer_score <= 21 {
            Ok(GameEnding::DealerWin)
        }else if dealer_score < player_score || dealer_score > 21{
            Ok(GameEnding::PlayerWin)
        }else {
            Ok(GameEnding::Tie)
        }
    }

    pub fn status(&self) -> String {
        format!("Cards in your hand: {} (Total Point: {}). \n\
                Cards (face-up) in my hand: {} (Total Point: {})", 
                self.player_cards.join(", "), self.sum_of(&self.player_cards), 
                self.dealer_cards.join(", "), self.sum_of(&self.dealer_cards))
    }

}

#[no_mangle]
#[tokio::main(flavor = "current_thread")]
pub async fn run() -> anyhow::Result<()> {
    let discord_token = std::env::var("discord_token").unwrap();
    let bot = ProvidedBot::new(discord_token);
    bot.listen(|msg| handler(&bot, msg)).await;
    Ok(())
}

async fn handler(bot: &ProvidedBot, msg: Message) {
    logger::init();
    let discord = bot.get_client();

    if msg.author.bot {                     // if the author is a bot
        log::debug!("ignored bot message");
        return;
    }
    if msg.member.is_some() {              // blackjack must be played in a server.
        log::debug!("ignored channel message");
        return;
    }

    if let Some(store) = get("bj"){
        let player_cards: Vec<String> = serde_json::from_value(store["player_cards"].clone()).unwrap();
        let dealer_cards: Vec<String> = serde_json::from_value(store["dealer_cards"].clone()).unwrap();
        let card2use: Vec<String> = serde_json::from_value(store["card2use"].clone()).unwrap();

        let mut game = Game {
            player_cards,
            dealer_cards,
            card2use,
        };
        
        let mut endding = false;
        let mut resp = match msg.content.to_lowercase().trim() {
            "hit" => match game.hit(){
                    Ok(HitOutcome::CONTINUE) => String::from("Order received."),
                    Ok(HitOutcome::BUST) => {endding = true; String::from(WINNER_MSG)},
                    Err(e) => {
                        endding = true;
                        format!("There's something wrong and the game will be terminated. Possible Reason: {}", e)
                    }
                },
            "stand" => {
                endding = true; 
                match game.stand() {
                    Ok(GameEnding::PlayerWin) => String::from(LOSER_MSG),
                    Ok(GameEnding::DealerWin) => String::from(WINNER_MSG),
                    Ok(GameEnding::Tie) => String::from(TIE_MSG),
                    Err(e) => format!("There's something wrong and the game will be terminated. Possible Reason: {}", e)
                }
            },
            "help" => String::from(HELP_MSG),
            "status" => String::from("So soon you forgot your card?"),
            _ => format!("I don't know what you mean. \n{}", String::from(HELP_MSG))
        };
        resp = format!("{}\n\n Current Status: \n{}", resp, game.status());
        let channel_id = msg.channel_id;
    
        _ = discord.send_message(
            channel_id.into(),
            &serde_json::json!({
                "content": resp
            }),
        ).await;

        if endding{
            del("bj");
        }else{
            set("bj", json!(game), None);
        }
    }else {
        if msg.content.to_lowercase().trim() != "blackjack"{
            log::debug!("ignored channel message");
            return;
        }
        let mut game = Game::new();
        let resp = match game.init_game(){
            Ok(_) => format!("Ok, let's begin. \n\n{}\n\nFor now,\n{}\n\n{}", INTRO_MSG, game.status(), HELP_MSG),
            Err(e) => format!("There's something wrong and the game will be terminated. Possible Reason: {}", e)
        };
        let channel_id = msg.channel_id;

        _ = discord.send_message(
            channel_id.into(),
            &serde_json::json!({
                "content": resp
            }),
        ).await;

        if game.sum_of(&game.player_cards) == 21{ 
            let channel_id = msg.channel_id;

            _ = discord.send_message(
                channel_id.into(),
                &serde_json::json!({
                    "content": String::from("I cannot believe it you got BLACKJACK!!!! You are so lucky this time! \n\
                                            Do you want another try? You may not be so lucky next time.")  
                }),
            ).await;
        }else{
            set("bj", json!(game), None);
        }
    }
    

}
