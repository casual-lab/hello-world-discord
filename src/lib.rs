use std::{collections::HashMap, ops::Index, default};

use anyhow::Ok;
use discord_flows::{
    model::Message,
    ProvidedBot, Bot,
};
use flowsnet_platform_sdk::logger;
use serde_json::json;
use store_flows::{get, set, Expire, ExpireKind};
use rand::seq::{SliceRandom, thread_rng};

static WINNER_MSG:String = String::from("Haha! I can't believe it! Looks like I completely crushed you, 
                            Don't worry, though, it's all in good fun. Maybe next time you'll stand a 
                            chance against my unbeatable skills. Until then, enjoy the taste of defeat!");
static LOSER_MSG: String = String::from("Alright, alright, you got me this time! 
                            I'll begrudgingly admit it, my friend you beat me fair and square. 
                            I'll be back for a rematch soon. Consider yourself lucky, my friend!");
static TIE_MSG: String = String::from("Ha! It's a tie, my friend! I must say, it's quite a 
                            rare occurrence. Shall we go at it again? ");
static HELP_MSG: String = String::from("There are 3 options for you: \n
                            1. By saying \"hit\", you can have another card\n
                            2. By saying \"stand\", you will take no more cards, so I will reveal the result.\n
                            3. By saying \"status\", you can know what cards in your hand as well as my face-up card.
                            4. By saying \"help\" or something else, you will see this help message.");
static INTRO_MSG: String = String::from("Let me introduce the rule of this game for you:\n
                            1. First I will give each of us two cards and one of mine is face-down;\n
                            2. After that, you can take another card by saying \"hit\" for many times 
                            until you stop by saying \"stand\". \n
                            3. If your point is beyond 21 after a hit, you lose immediately\n
                            4. After you \"stand\", I will reveal the face-down card and take card until my point goes beyond 17. \n
                            5. If your point is less than 21 and greater than mine, or my point is greater than 21, you are the winner. \n
                            6. If my point is less than 21 and greater than yours, I win. (You will lose in step 3 if yours is greater than 21.)\n
                            7. In other situations, it's a tie.");

type Card = String;

struct Game{
    pub dealer_cards: Vec<Card>,
    pub player_cards: Vec<Card>,

    card2use: Vec<String>
}

impl Game {
    fn sum_of(&self, cards: &Vec<char>) -> u64{
        let mut sum:i64 = 0;
        for c in cards {
            let v = match c {
                "2"|"3"|"4"|"5"|"6"|"7"|"8"|"9" => c.to_digit(10).unwrap(),
                "10"|"J"|"Q"|"K" => 10,
                'A' => 11
            };
            sum += c;
        }
        sum
    }
    fn pop_one_card(&self) -> Result<u8, String> {
        if let Some(next) = self.card2use.first().clone(){
            self.card2use.remove(0);
            Ok(next)
        }else{
            Err(String("Insufficient Cards"))
        }
    }
    pub fn init_game(&self) -> Result<(), String> {
        for _ in 0..2{
            self.player_cards.push(self.pop_one_card()?);
        }
        self.dealer_cards.push(self.pop_one_card()?);
        Ok(())
    }

    pub fn new() -> Game {
        let one_deck = vec!["2", "3", "4", "5", "6", "7", "8", "9", "10", "J", "Q", "K", "A"];
        let one_deck: Vec<String> = one_deck.iter().map(|&s|s.to_owned()).collect();
        one_deck = Vec::new();
        for _ in 0..4 {
            one_deck.extend(one_deck);
        }
        one_deck.extend(one_deck);
        let mut rng = thread_rng();
        one_deck.shuffle(&mut rng);

        Game { dealer_cards: Vec::new(), player_cards: Vec::new(), card2use: one_deck }
    }

    pub fn hit(&self) -> Result<bool, String>{
        self.player_cards.push(self.pop_one_card()?);
        if self.sum_of(cards) > 21{
            return Ok(true)
        }
        Ok(false)
    }

    pub fn stand(&self) -> Option<bool>{
        self.dealer_cards.push(self.pop_one_card());
        while self.sum_of(&self.dealer_cards) < 17{
            self.dealer_cards.push(self.pop_one_card());
        }

        let dealer_score = self.sum_of(&self.dealer_cards);
        let player_score = self.sum_of(&self.player_cards);
        if player_score > 21 || player_score < dealer_score {
            Some(false)
        }else if dealer_score < player_score || dealer_score > 21{
            Some(true)
        }else {
            None
        }
    }

    pub fn status(&self) -> String {
        format!("Cards in your hand: {}. \n
                Cards (face-up) in my hand: {}", 
                self.player_cards.join(", "), self.dealer_cards.join(", "))
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

    if let Some(state_store) = get("bj"){
        let game = Game{
            player_cards: state_store.as_obj()["player_cards"].copy(),
            dealer_cards: state_store.as_obj()["dealer_cards"].copy(),
            card2use: state_store.as_obj()["card2use"].copy()
        };
        let resp = match msg.content.to_lowercase().trim() {
            "hit" => match game.hit(){
                    Ok(true) => String::from("Order received."),
                    Err(e) => format!("There's something wrong and the game will be terminated. Possible Reason: {}", e)
                },
            "stand" => match game.stand() {
                Some(true) => LOSER_MSG,
                Some(false) => WINNER_MSG,
                None => TIE_MSG
            },
            "help" => HELP_MSG,
            "status" => &game.status(),
            default => format!("I don't know what you mean. \n{}", HELP_MSG)
        };
        resp = format!("{}\n Current Status: \n{}", resp, game.status());
        let channel_id = msg.channel_id;
    
        _ = discord.send_message(
            channel_id.into(),
            &serde_json::json!({
                "content": resp
            }),
        ).await;

        set(
            "bj", json!(game), None
        );
    }else {
        if msg.content.to_lowercase().trim() == "blackjack"{
            let game = Game::new();
            match game.init_game(){
                Ok(_) => format!("Ok, let's begin. \n{}\nFor now,\n{}\n{}", INTRO_MSG, game.status(), HELP_MSG),
                Err(e) => format!("There's something wrong and the game will be terminated. Possible Reason: {}", e)
            }
        }
    }

}
