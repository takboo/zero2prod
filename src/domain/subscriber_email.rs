use validator::Validate;

#[derive(Debug, Validate)]
pub struct SubscriberEmail {
    #[validate(email)]
    email: String,
}

impl AsRef<str> for SubscriberEmail {
    fn as_ref(&self) -> &str {
        &self.email
    }
}

impl TryFrom<String> for SubscriberEmail {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let subscriber_email = Self { email: value };
        match subscriber_email.validate() {
            Ok(_) => Ok(subscriber_email),
            Err(_) => Err(format!(
                "'{}' is not a valid subscriber email",
                subscriber_email.email
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SubscriberEmail;
    use claims::{assert_err, assert_ok};
    use fake::Fake;
    use fake::faker::internet::en::SafeEmail;
    use fake::rand::SeedableRng;
    use fake::rand::rngs::StdRng;
    use proptest::prelude::{Strategy, any, proptest};

    fn valid_email() -> impl Strategy<Value = String> {
        any::<u64>().prop_map(|seed| {
            let mut rng = StdRng::seed_from_u64(seed);
            SafeEmail().fake_with_rng(&mut rng)
        })
    }

    #[test]
    fn valid_emails_are_parsed_successfully() {
        let email: String = SafeEmail().fake();
        assert_ok!(SubscriberEmail::try_from(email));
    }

    proptest! {
        #[test]
        fn valid_emails_are_accepted(email in valid_email()) {
            SubscriberEmail::try_from(email).unwrap();
        }
    }

    #[test]
    fn empty_string_is_rejected() {
        let email = "".to_string();
        assert_err!(SubscriberEmail::try_from(email));
    }

    #[test]
    fn email_missing_at_symbol_is_rejected() {
        let email = "ursuladomain.com".to_string();
        assert_err!(SubscriberEmail::try_from(email));
    }

    #[test]
    fn email_missing_subject_is_rejected() {
        let email = "@domain.com".to_string();
        assert_err!(SubscriberEmail::try_from(email));
    }
}
