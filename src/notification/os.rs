use std::process::Command;

pub struct OsNotifier {
    pub sound: bool,
}

impl OsNotifier {
    pub fn send(&self, title: &str, body: &str) {
        let _ = Command::new("notify-send")
            .arg(title)
            .arg(body)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();

        if self.sound {
            let _ = Command::new("paplay")
                .arg("/usr/share/sounds/freedesktop/stereo/complete.oga")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        }
    }
}
