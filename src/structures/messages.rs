use std::vec::IntoIter;
use std::thread;
use std::time::Duration;
use serde_json;


use crate::client::RedditClient;
use crate::traits::{Created, Content, Approvable, PageListing, Editable, Commentable};
use crate::structures::user::User;
use crate::structures::comment::Comment;
use crate::responses::comment::NewComment;
use crate::structures::comment_list::CommentList;
use crate::errors::APIError;
use crate::structures::subreddit::Subreddit;
use crate::options::ListingOptions;
use crate::responses::listing;
use crate::responses::messages::{MessageData, MessageListingData};
use async_trait::async_trait;

/// A representation of a private message from Reddit.
pub struct Message<'a> {
    client: &'a RedditClient,
    data: MessageData,
}

impl<'a> Message<'a> {
    /// Internal method. Use `RedditClient.messages().inbox()` or `unread()` instead to get
    /// message listings and individual messages.
    pub fn new(client: &RedditClient, data: MessageData) -> Message {
        Message {
            client: client,
            data: data,
        }
    }

    /// Gets the full name (kind + id, e.g. 't1_a5bzp') of the parent of this submission.
    pub fn parent_id(&self) -> Option<String> {
        self.data.parent_id.to_owned()
    }

    /// Marks this message as read, so it will not show in the unread queue.
    pub async fn mark_read(&self) -> Result<(), APIError> {
        let body = format!("id={}", self.name());
        self.client.post_success("/api/read_message", &body, false).await
    }
}
#[async_trait]
impl<'a> Commentable<'a> for Message<'a> {
    fn reply_count(&self) -> u64 {
        panic!("The Reddit API does not appear to return the reply count to messages, so this \
                function is unavailable.");
    }

   async fn replies(self) -> Result<CommentList<'a>, APIError> {
        panic!("The Reddit API does not seem to return replies to messages as expected, so this \
                function is unavailable.");
    }

    async fn reply(&self, text: &str) -> Result<Comment, APIError> {
        let body = format!("api_type=json&text={}&thing_id={}",
                           self.client.url_escape(text.to_owned()),
                           self.name());
        let result = self.client.post_json("/api/comment", &body, false).await.unwrap();
        let result :NewComment = serde_json::from_str(&*result).unwrap();
        Ok(Comment::new(self.client, result.json.data.things.into_iter().next().unwrap().data))


    }
}

impl<'a> Created for Message<'a> {
    fn created(&self) -> i64 {
        self.data.created as i64
    }

    fn created_utc(&self) -> i64 {
        self.data.created_utc as i64
    }
}
#[async_trait]
impl<'a> Content for Message<'a> {
    fn author(&self) -> User {
        let author = self.data.author.to_owned().unwrap_or(String::from("reddit"));
        User::new(self.client, &author)
    }

    fn author_flair_text(&self) -> Option<String> {
        None
    }

    fn author_flair_css(&self) -> Option<String> {
        None
    }

    fn subreddit(&self) -> Subreddit {
        let subreddit = self.data.subreddit.to_owned().unwrap_or(String::from("all"));
        Subreddit::create_new(self.client, &subreddit)
    }

    async fn delete(self) -> Result<(), APIError> {
        let body = format!("id={}", self.data.name);
        self.client.post_success("/api/del_msg", &body, false).await
    }

    fn name(&self) -> &str {
        &self.data.name
    }
}
#[async_trait]
impl<'a> Approvable for Message<'a> {
   async fn approve(&self) -> Result<(), APIError> {
        let body = format!("id={}", self.data.name);
        self.client.post_success("/api/approve", &body, false).await
    }

    async fn remove(&self, spam: bool) -> Result<(), APIError> {
        let body = format!("id={}&spam={}", self.data.name, spam);
        self.client.post_success("/api/remove", &body, false).await
    }

    async  fn ignore_reports(&self) -> Result<(), APIError> {
        let body = format!("id={}", self.data.name);
        self.client.post_success("/api/ignore_reports", &body, false).await
    }

    async  fn unignore_reports(&self) -> Result<(), APIError> {
        let body = format!("id={}", self.data.name);
        self.client.post_success("/api/unignore_reports", &body, false).await
    }
}
#[async_trait]
impl<'a> Editable for Message<'a> {
    fn edited(&self) -> bool {
        panic!("Reddit does not provide access to the edit time for messages.");
    }

    fn edited_time(&self) -> Option<i64> {
        panic!("Reddit does not provide access to the edit time for messages.");
    }

    async fn edit(&mut self, text: &str) -> Result<(), APIError> {
        let body = format!("api_type=json&text={}&thing_id={}",
                           self.client.url_escape(text.to_owned()),
                           self.data.name);
        let res = self.client.post_success("/api/editusertext", &body, false).await;
        if let Ok(()) = res {
            // TODO: should we update body_html?
            self.data.body = text.to_owned();
        }
        res
    }

    fn body(&self) -> Option<String> {
        Some(self.data.body.to_owned())
    }

    fn body_html(&self) -> Option<String> {
        Some(self.data.body_html.to_owned())
    }
}

/// A helper struct which allows access to the inbox, unread messages and other message queues.
pub struct MessageInterface<'a> {
    client: &'a RedditClient,
}

impl<'a> MessageInterface<'a> {
    /// Internal method. Use `RedditClient.messages()` instead.
    pub fn new(client: &RedditClient) -> MessageInterface {
        MessageInterface { client: client }
    }

    /// Composes a private message to send to a user.
    /// # Examples
    /// ```ignore
    ///
    /// let client = RedditClient::new("new_rawr", AnonymousAuthenticator::new());
    /// client.messages().compose("Aurora0001", "Test", "Hi!");
    // ```
    pub async fn compose(&self, recipient: &str, subject: &str, body: &str) -> Result<(), APIError> {
        let body = format!("api_type=json&subject={}&text={}&to={}", subject, body, recipient);
        self.client.post_success("/api/compose", &body, false).await
    }

    /// Gets a list of all received messages that have not been deleted.
    pub async fn inbox(&self, opts: ListingOptions) -> Result<MessageListing<'a>, APIError> {
        let uri = format!("/message/inbox?raw_json=1&limit={}", opts.batch);
        let full_uri = format!("{}&{}", uri, opts.anchor);
        let result = self.client
            .get_json(&full_uri, false).await.unwrap();
        let result :MessageListingData = serde_json::from_str(&*result).unwrap();
        Ok(MessageListing::new(self.client, uri, result.data))
    }

    /// Gets all messages that have **not** been marked as read.
    pub async fn unread(&self, opts: ListingOptions) -> Result<MessageListing<'a>, APIError> {
        let uri = format!("/message/unread?raw_json=1&limit={}", opts.batch);
        let full_uri = format!("{}&{}", uri, opts.anchor);
        let result = self.client
            .get_json(&full_uri, false).await.unwrap();
        let result :MessageListingData = serde_json::from_str(&*result).unwrap();
        Ok(MessageListing::new(self.client, uri, result.data))

    }


}

// TODO: refactor Listing to cover this case too.

/// A listing of messages that will auto-paginate until all messages in the listing have been
/// exhausted.
pub struct MessageListing<'a> {
    client: &'a RedditClient,
    query_stem: String,
    data: listing::ListingData<MessageData>,
}

impl<'a> MessageListing<'a> {
    /// Internal method. Use `RedditClient.messages()` and request one of the message listings
    /// (e.g. `inbox(LISTING_OPTIONS)`).
    pub fn new(client: &RedditClient,
               query_stem: String,
               data: listing::ListingData<MessageData>)
               -> MessageListing {
        MessageListing {
            client: client,
            query_stem: query_stem,
            data: data,
        }
    }
}

impl<'a> PageListing for MessageListing<'a> {
    fn before(&self) -> Option<String> {
        self.data.before.to_owned()
    }

    fn after(&self) -> Option<String> {
        self.data.after.to_owned()
    }

    fn modhash(&self) -> Option<String> {
        self.data.modhash.to_owned()
    }
}

impl<'a> MessageListing<'a> {
    async fn fetch_after(&mut self) -> Result<MessageListing<'a>, APIError> {
        match self.after() {
            Some(after_id) => {
                let url = format!("{}&after={}", self.query_stem, after_id);
                let string = self.client
                    .get_json(&url, false).await.unwrap();
                let string:MessageListingData = serde_json::from_str(&*string).unwrap();
                Ok(MessageListing::new(self.client, self.query_stem.to_owned(), string.data))
            }
            None => Err(APIError::ExhaustedListing),
        }
    }
}

