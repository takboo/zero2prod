use crate::domain::SubscriberEmail;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

pub struct EmailClient {
    http_client: reqwest::Client,
    base_url: String,
    sender: SubscriberEmail,
    authorization_token: SecretString,
}

impl EmailClient {
    pub fn new(
        base_url: String,
        sender: SubscriberEmail,
        authorization_token: SecretString,
        timeout_duration: std::time::Duration,
    ) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(timeout_duration)
            .build()
            .unwrap();

        Self {
            http_client,
            base_url,
            sender,
            authorization_token,
        }
    }

    pub async fn send_email(
        &self,
        recipient: &SubscriberEmail,
        subject: &str,
        html_content: &str,
        text_content: &str,
    ) -> Result<(), reqwest::Error> {
        let url = format!("{}/api/send", self.base_url);
        let sender = EmailInfo {
            email: self.sender.as_ref(),
            name: "",
        };
        let to = EmailInfo {
            email: recipient.as_ref(),
            name: "",
        };
        let request_body = SendEmailRequest {
            subject: subject.into(),
            from: sender,
            to: vec![to],
            text: text_content.into(),
            html: html_content.into(),
            category: "".into(),
        };
        self.http_client
            .post(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.authorization_token.expose_secret()),
            )
            .json(&request_body)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EmailInfo<'a> {
    pub email: &'a str,
    pub name: &'a str,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SendEmailRequest<'a> {
    pub from: EmailInfo<'a>,
    pub to: Vec<EmailInfo<'a>>,
    #[serde(borrow)]
    pub subject: Cow<'a, str>,
    #[serde(borrow)]
    pub text: Cow<'a, str>,
    #[serde(borrow)]
    pub html: Cow<'a, str>,
    #[serde(borrow)]
    pub category: Cow<'a, str>,
}

#[cfg(test)]
mod tests {
    use crate::EmailClient;
    use crate::domain::SubscriberEmail;
    use claims::{assert_err, assert_ok};
    use fake::faker::internet::en::SafeEmail;
    use fake::faker::lorem::en::{Paragraph, Sentence};
    use fake::{Fake, Faker};
    use secrecy::{SecretBox, SecretString};
    use wiremock::matchers::{any, header, header_exists, method, path};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};
    struct SendEmailBodyMatcher;

    impl wiremock::Match for SendEmailBodyMatcher {
        fn matches(&self, request: &Request) -> bool {
            let result: Result<super::SendEmailRequest, _> = serde_json::from_slice(&request.body);
            result.is_ok()
        }
    }

    /// Generate a random email address
    fn email() -> SubscriberEmail {
        SafeEmail().fake::<String>().try_into().unwrap()
    }

    /// Generate a random email subject
    fn subject() -> String {
        Sentence(1..2).fake()
    }

    /// Generate a random email content
    fn content() -> String {
        Paragraph(10..20).fake()
    }

    /// Generate a random token for authorization
    fn token() -> SecretString {
        SecretBox::new(Faker.fake::<String>().into())
    }

    /// Get a test instance of `EmailClient`
    fn email_client(base_url: String) -> EmailClient {
        EmailClient::new(
            base_url,
            email(),
            token(),
            std::time::Duration::from_millis(200),
        )
    }

    #[tokio::test]
    async fn send_email_fires_a_request_to_base_url() {
        let mock_server = MockServer::start().await;
        let email_client = email_client(mock_server.uri());

        Mock::given(header_exists("authorization"))
            .and(header("content-type", "application/json"))
            .and(path("/api/send"))
            .and(method("POST"))
            .and(SendEmailBodyMatcher)
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock_server)
            .await;

        let _ = email_client
            .send_email(&email(), &subject(), &content(), &content())
            .await;
    }

    #[tokio::test]
    async fn send_email_succeeds_if_the_server_returns_200() {
        // Arrange
        let mock_server = MockServer::start().await;
        let email_client = email_client(mock_server.uri());

        Mock::given(any())
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock_server)
            .await;

        // Act
        let outcome = email_client
            .send_email(&email(), &subject(), &content(), &content())
            .await;

        // Assert
        assert_ok!(outcome);
    }

    #[tokio::test]
    async fn send_email_fails_if_the_server_returns_500() {
        // Arrange
        let mock_server = MockServer::start().await;
        let email_client = email_client(mock_server.uri());

        Mock::given(any())
            .respond_with(ResponseTemplate::new(500))
            .expect(1)
            .mount(&mock_server)
            .await;

        // Act
        let outcome = email_client
            .send_email(&email(), &subject(), &content(), &content())
            .await;

        // Assert
        assert_err!(outcome);
    }

    #[tokio::test]
    async fn send_email_times_out_if_the_server_takes_too_long() {
        // Arrange
        let mock_server = MockServer::start().await;
        let email_client = email_client(mock_server.uri());

        let response = ResponseTemplate::new(200)
            // 3 minutes!
            .set_delay(std::time::Duration::from_secs(180));
        Mock::given(any())
            .respond_with(response)
            .expect(1)
            .mount(&mock_server)
            .await;

        // Act
        let outcome = email_client
            .send_email(&email(), &subject(), &content(), &content())
            .await;

        // Assert
        assert_err!(outcome);
    }
}
