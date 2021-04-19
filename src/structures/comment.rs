use serde_json;
use serde_json::from_value;

use crate::client::RedditClient;
use crate::structures::comment_list::CommentList;
use crate::traits::{Votable, Created, Editable, Content, Commentable, Approvable, Stickable, Distinguishable, Reportable};
use crate::errors::APIError;
use crate::responses::comment::{CommentData};
use crate::structures::user::User;
use crate::structures::subreddit::Subreddit;
use crate::responses::comment::{NewComment, CommentListing};
use async_trait::async_trait;

/// Structure representing a comment and its associated data (e.g. replies)
pub struct Comment<'a> {
    data: CommentData,
    client: &'a RedditClient,
    replies: CommentList<'a>,
}

#[async_trait]
impl<'a> Votable for Comment<'a> {
    fn score(&self) -> i64 {
        self.data.score
    }

    fn likes(&self) -> Option<bool> {
        self.data.likes
    }

    async fn upvote(&self) -> Result<(), APIError> {
        self.vote(1)
    }

    async fn downvote(&self) -> Result<(), APIError> {
        self.vote(-1)
    }

    async fn cancel_vote(&self) -> Result<(), APIError> {
        self.vote(0)
    }
}

impl<'a> Created for Comment<'a> {
    fn created(&self) -> i64 {
        self.data.created as i64
    }

    fn created_utc(&self) -> i64 {
        self.data.created_utc as i64
    }
}

#[async_trait]
impl<'a> Editable for Comment<'a> {
    fn edited(&self) -> bool {
        self.data.edited.as_bool().unwrap()
    }

    fn edited_time(&self) -> Option<i64> {
        self.data.edited.as_i64()
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

#[async_trait]
impl<'a> Content for Comment<'a> {
    fn author(&self) -> User {
        User::new(self.client, &self.data.author)
    }

    fn author_flair_text(&self) -> Option<String> {
        self.data.author_flair_text.to_owned()
    }

    fn author_flair_css(&self) -> Option<String> {
        self.data.author_flair_css_class.to_owned()
    }

    fn subreddit(&self) -> Subreddit {
        Subreddit::create_new(self.client, &self.data.subreddit)
    }

    async fn delete(self) -> Result<(), APIError> {
        let body = format!("id={}", self.data.name);
        self.client.post_success("/api/del", &body, false).await
    }

    fn name(&self) -> &str {
        &self.data.name
    }
}

#[async_trait]
impl<'a> Approvable for Comment<'a> {
    async fn approve(&self) -> Result<(), APIError> {
        let body = format!("id={}", self.data.name);
        self.client.post_success("/api/approve", &body, false)
    }

    async fn remove(&self, spam: bool) -> Result<(), APIError> {
        let body = format!("id={}&spam={}", self.data.name, spam);
        self.client.post_success("/api/remove", &body, false)
    }

    async fn ignore_reports(&self) -> Result<(), APIError> {
        let body = format!("id={}", self.data.name);
        self.client.post_success("/api/ignore_reports", &body, false)
    }

    async fn unignore_reports(&self) -> Result<(), APIError> {
        let body = format!("id={}", self.data.name);
        self.client.post_success("/api/unignore_reports", &body, false)
    }
}

#[async_trait]
impl<'a> Commentable<'a> for Comment<'a> {
    fn reply_count(&self) -> u64 {
        panic!("There is no effective way of getting the number of comment replies. You may have \
                to manually count with `replies().len()`, which may take some time.");
    }

    async fn reply(&self, text: &str) -> Result<Comment, APIError> {
        let body = format!("api_type=json&text={}&thing_id={}",
                           self.client.url_escape(text.to_owned()),
                           self.name());
        let result = self.client.post_json("/api/comment", &body, false).unwrap();
        let result: NewComment = serde_json::from_str(&*result).unwrap();
        Ok(Comment::new(self.client, result.json.data.things.into_iter().next().unwrap().data))
    }

    async fn replies(self) -> Result<CommentList<'a>, APIError> {
        Ok(self.replies)
    }
}

impl<'a> Comment<'a> {
    /// Internal method. Use `Submission.replies()` or `Comment.replies()` to get a listing, then
    /// select the desired comment instead.
    pub fn new(client: &RedditClient, data: CommentData) -> Comment {
        let comments = if data.replies.is_object() {
            // TODO: avoid cloning here
            let listing = from_value::<CommentListing>(data.replies.clone()).unwrap();
            CommentList::new(client,
                             data.link_id.to_owned(),
                             data.name.to_owned(),
                             listing.data.children)
        } else {
            CommentList::empty(client)
        };

        Comment {
            client: client,
            data: data,
            replies: comments,
        }
    }

    /// Gets the full ID of the parent submission/comment (kind + id e.g. 't1_4te6jf')
    pub fn parent(&self) -> &str {
        &self.data.parent_id
    }

    /// Adds a reply to this comment's reply list. This is an internal method - to make the client
    /// reply to this post, use `Comment.reply(MESSAGE)`.
    pub fn add_reply(&mut self, item: Comment<'a>) {
        self.replies.add_reply(item);
    }

    fn vote(&self, dir: i8) -> Result<(), APIError> {
        let body = format!("dir={}&id={}", dir, self.data.name);
        self.client.post_success("/api/vote", &body, false)
    }
}

#[async_trait]
impl<'a> Reportable for Comment<'a> {
    async fn report(&self, reason: &str) -> Result<(), APIError> {
        let body = format!("api_type=json&thing_id={}&reason={}",
                           self.data.name,
                           self.client.url_escape(reason.to_owned()));
        self.client.post_success("/api/report", &body, false)
    }

    fn report_count(&self) -> Option<u64> {
        self.data.num_reports.to_owned()
    }
}

#[async_trait]
impl<'a> Stickable for Comment<'a> {
    fn stickied(&self) -> bool {
        self.data.stickied
    }

    async fn stick(&mut self) -> Result<(), APIError> {
        let body = format!("api_type=json&how=yes&sticky=true&id={}", self.data.name);
        let res = self.client.post_success("/api/distinguish", &body, false);
        if let Ok(()) = res {
            self.data.stickied = true;
        }
        res
    }

    async fn unstick(&mut self) -> Result<(), APIError> {
        let body = format!("api_type=json&how=no&id={}", self.data.name);
        let res = self.client.post_success("/api/distinguish", &body, false);
        if let Ok(()) = res {
            self.data.stickied = false;
        }
        res
    }
}

#[async_trait]
impl<'a> Distinguishable for Comment<'a> {
    fn distinguished(&self) -> Option<String> {
        self.data.distinguished.to_owned()
    }

    async fn distinguish(&mut self) -> Result<(), APIError> {
        let body = format!("api_type=json&how=yes&id={}", self.data.name);
        let res = self.client.post_success("/api/distinguish", &body, false);
        if let Ok(()) = res {
            self.data.distinguished = Some(String::from("moderator"));
        }
        res
    }

    async fn undistinguish(&mut self) -> Result<(), APIError> {
        let body = format!("api_type=json&how=no&id={}", self.data.name);
        let res = self.client.post_success("/api/distinguish", &body, false);
        if let Ok(()) = res {
            self.data.distinguished = None;
        }
        res
    }
}
