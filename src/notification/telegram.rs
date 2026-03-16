pub struct TelegramNotifier {
    bot_token: String,
    chat_id: String,
}

impl TelegramNotifier {
    pub fn new(bot_token: String, chat_id: String) -> Self {
        Self { bot_token, chat_id }
    }

    /// Send a message via the Telegram Bot API in a spawned thread.
    pub fn send(&self, title: &str, body: &str) {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.bot_token,
        );
        let text = format!("*{}*\n{}", title, body);
        let chat_id = self.chat_id.clone();

        std::thread::spawn(move || {
            let result = ureq::post(&url)
                .send_json(ureq::json!({
                    "chat_id": chat_id,
                    "text": text,
                    "parse_mode": "Markdown",
                }));
            if let Err(e) = result {
                crate::humu_log!("telegram notification failed: {e}");
            }
        });
    }
}
