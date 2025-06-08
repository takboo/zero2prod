use crate::domain::{SubscriberEmail, SubscriberName};
use crate::routes::subscriptions::FormData;

#[derive(Debug)]
pub struct NewSubscriber {
    pub email: SubscriberEmail,
    pub name: SubscriberName,
}

impl TryFrom<FormData> for NewSubscriber {
    type Error = String;

    fn try_from(form: FormData) -> Result<Self, Self::Error> {
        let name = SubscriberName::try_from(form.name)?;
        let email = SubscriberEmail::try_from(form.email)?;
        Ok(Self { email, name })
    }
}
