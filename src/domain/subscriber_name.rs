use unicode_segmentation::UnicodeSegmentation;
use validator::{Validate, ValidationError};

fn validate_subscriber_name(s: &str) -> Result<(), ValidationError> {
    if s.trim().is_empty() {
        return Err(ValidationError::new("Subscriber name cannot be empty"));
    }
    if s.graphemes(true).count() > 256 {
        return Err(ValidationError::new(
            "Subscriber name cannot be longer than 256 characters",
        ));
    }
    let forbidden_characters = [
        '/', '(', ')', '[', ']', '{', '}', '"', '<', '>', '\\', '|', '`', '$', ';', ':', '.', ',',
    ];
    if s.chars().any(|c| forbidden_characters.contains(&c)) {
        return Err(ValidationError::new(
            "Subscriber name cannot contain forbidden characters",
        ));
    }
    Ok(())
}

#[derive(Debug, Validate)]
pub struct SubscriberName {
    #[validate(custom(function = "validate_subscriber_name"))]
    pub name: String,
}

impl AsRef<str> for SubscriberName {
    fn as_ref(&self) -> &str {
        &self.name
    }
}

impl TryFrom<String> for SubscriberName {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let s = SubscriberName { name: value };
        match s.validate() {
            Ok(_) => Ok(s),
            Err(_) => Err(format!("{} is not a valid subscriber name", s.name)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SubscriberName;
    use claims::{assert_err, assert_ok};

    #[test]
    fn a_256_grapheme_long_name_is_valid() {
        let name = "a̐".repeat(256);
        assert_ok!(SubscriberName::try_from(name));
    }

    #[test]
    fn a_name_longer_than_256_graphemes_is_rejected() {
        let name = "a".repeat(257);
        assert_err!(SubscriberName::try_from(name));
    }

    #[test]
    fn whitespace_only_names_are_rejected() {
        let name = " ".to_string();
        assert_err!(SubscriberName::try_from(name));
    }

    #[test]
    fn empty_string_is_rejected() {
        let name = "".to_string();
        assert_err!(SubscriberName::try_from(name));
    }

    #[test]
    fn names_containing_an_invalid_character_are_rejected() {
        let forbidden_characters = [
            '/', '(', ')', '[', ']', '{', '}', '"', '<', '>', '\\', '|', '`', '$', ';', ':', '.',
            ',',
        ];
        for name in forbidden_characters {
            let name = name.to_string();
            assert_err!(SubscriberName::try_from(name));
        }
    }

    #[test]
    fn a_valid_name_is_parsed_successfully() {
        let name = "Ursula Le Guin".to_string();
        assert_ok!(SubscriberName::try_from(name));
    }
}
