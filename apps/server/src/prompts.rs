//! Prompt text for the model. Pure functions, no I/O.

/// Single user message: with a system prompt lfm3-audio often returns "".
pub fn translate(text: &str, from: &str, to: &str) -> String {
    let (fc, tc) = (code(from), code(to));
    format!(
        "You are a professional {from} ({fc}) to {to} ({tc}) translator. Your goal is to accurately convey the meaning and nuances of the original {from} text while adhering to {to} grammar, vocabulary, and cultural sensitivities.\n\
         Produce only the {to} translation, without any additional explanations or commentary. Please translate the following {from} text into {to}:\n\n\n\
         {text}"
    )
}

/// Goes inside the latest user turn: with a system prompt lfm3-audio often
/// returns "" or drifts into English.
pub fn chat(text: &str, target: &str, level: &str) -> String {
    format!(
        "You are a friendly AI assistant having a natural conversation with the user. \
         The user says: \"{text}\"\n\n\
         Always reply only in {target}, even if the user writes in another language. Never use any other language. \
         Output only your reply. {level}",
        level = level.to_lowercase()
    )
}

/// ISO 639-1 code for the UI's language names (LANGS in web/app.js). Unknown names pass through.
pub fn code(lang: &str) -> &str {
    match lang {
        "English" => "en",
        "Spanish" => "es",
        "French" => "fr",
        "German" => "de",
        "Italian" => "it",
        "Portuguese" => "pt",
        "Japanese" => "ja",
        "Chinese" => "zh",
        "Korean" => "ko",
        "Arabic" => "ar",
        "Dutch" => "nl",
        "Danish" => "da",
        "Swedish" => "sv",
        "Finnish" => "fi",
        "Polish" => "pl",
        "Greek" => "el",
        "Hebrew" => "he",
        "Turkish" => "tr",
        "Tagalog" => "tl",
        _ => lang,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_maps_ui_languages_to_iso_639_1() {
        assert_eq!(code("English"), "en");
        assert_eq!(code("Spanish"), "es");
        assert_eq!(code("Chinese"), "zh");
        assert_eq!(code("Hebrew"), "he");
        assert_eq!(code("Tagalog"), "tl");
    }

    #[test]
    fn code_passes_unknown_languages_through() {
        assert_eq!(code("Klingon"), "Klingon");
    }

    #[test]
    fn translate_prompt_names_both_languages_with_codes_and_ends_with_text() {
        let p = translate("¿Qué tal?", "Spanish", "English");
        assert!(p.starts_with(
            "You are a professional Spanish (es) to English (en) translator."
        ));
        assert!(p.contains("Produce only the English translation"));
        assert!(p.contains("translate the following Spanish text into English:"));
        assert!(p.ends_with("\n\n\n¿Qué tal?"));
    }

    #[test]
    fn chat_prompt_quotes_user_and_pins_target_language_and_level() {
        let p = chat("hola", "French", "Beginner");
        assert!(p.contains("The user says: \"hola\""));
        assert!(p.contains("Always reply only in French"));
        assert!(p.ends_with("beginner"));
    }
}
