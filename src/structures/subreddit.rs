#![allow(unknown_lints, wrong_self_convention, new_ret_no_self)]

use crate::client::RedditClient;
use crate::options::{ListingOptions, TimeFilter, LinkPost, SelfPost};
use crate::structures::listing::Listing;
use crate::responses::listing;
use crate::traits::Created;
use crate::errors::APIError;
use hyper::Body;
use crate::structures::user::UserListing;
use std::error::Error;
use serde_json::Value;
use std::str::FromStr;
use async_trait::async_trait;

/// The `Subreddit` struct represents a subreddit and allows access to post listings
/// and data about the subreddit.
pub struct Subreddit<'a> {
    /// The name of the subreddit represented by this struct.
    pub name: String,
    client: &'a RedditClient,
}

impl<'a> PartialEq for Subreddit<'a> {
    fn eq(&self, other: &Subreddit) -> bool {
        self.name == other.name
    }
}

impl<'a> Subreddit<'a> {
    async fn get_feed(&self, ty: &str, opts: ListingOptions) -> Result<Listing<'_>, APIError> {
        // We do not include the after/before parameter here so the pagination can adjust it later
        // on.
        let uri = format!("/r/{}/{}limit={}&raw_json=1", self.name, ty, opts.batch);
        let full_uri = format!("{}&{}", uri, opts.anchor);
        let string = self.client
            .get_json(&full_uri, false).await.unwrap();
        let string: listing::Listing = serde_json::from_str(&*string).unwrap();
        Ok(Listing::new(self.client, uri, string.data))
    }

    /// Creates a `Subreddit` from a client and the subreddit's name. Do not use this directly -
    /// use `Client.subreddit(NAME)` instead.
    pub fn create_new(client: &'a RedditClient, name: &str) -> Subreddit<'a> {
        Subreddit {
            client: client,
            name: name.to_owned(),
        }
    }

    /// Gets a listing of the hot feed for this subreddit. The first page may include some sticky
    /// posts in addtion to the expected posts.
    /// # Examples
    /// ```
    /// use new_rawr::client::RedditClient;
    /// use new_rawr::options::ListingOptions;
    /// use new_rawr::auth::AnonymousAuthenticator;
    /// let client = RedditClient::new("new_rawr", AnonymousAuthenticator::new());
    /// let sub = client.subreddit("askreddit");
    /// let hot = sub.hot(ListingOptions::default());
    /// ```
    pub async fn hot(&self, opts: ListingOptions) -> Result<Listing<'_>, APIError> {
        self.get_feed("hot?", opts).await
    }



    /// Gets a listing of the new feed for this subreddit.
    /// # Examples
    /// ```
    /// use new_rawr::client::RedditClient;
    /// use new_rawr::options::ListingOptions;
    /// use new_rawr::traits::Content;
    /// use new_rawr::auth::AnonymousAuthenticator;
    /// let client = RedditClient::new("new_rawr", AnonymousAuthenticator::new());
    /// let sub = client.subreddit("programming");
    /// let mut new = sub.new(ListingOptions::default()).expect("Could not get new feed");
    /// assert_eq!(new.next().unwrap().subreddit().name, "programming");
    /// ```
    pub async fn new(&self, opts: ListingOptions) -> Result<Listing<'_>, APIError> {
        self.get_feed("new?", opts).await
    }

    /// Gets a listing of the rising feed for this subreddit. Usually much shorter than the other
    /// listings; may be empty.
    /// # Examples
    /// ```ignore
    /// use new_rawr::client::RedditClient;
    /// use new_rawr::options::ListingOptions;
    /// use new_rawr::auth::AnonymousAuthenticator;
    /// let client = RedditClient::new("new_rawr", AnonymousAuthenticator::new());
    /// let sub = client.subreddit("thanksobama");
    /// let rising = sub.rising(ListingOptions::default()).unwrap();
    /// assert_eq!(rising.count(), 0);
    /// ```
    pub async fn rising(&self, opts: ListingOptions) -> Result<Listing<'_>, APIError> {
        self.get_feed("rising?", opts).await
    }


    /// Gets a listing of the top feed for this subreddit. Also requires a time filter (
    /// `new_rawr::options::TimeFilter`) which is equivalent to the "links from: all time" dropdown
    /// on the website.
    /// # Examples
    /// ```ignore
    /// use new_rawr::client::RedditClient;
    /// use new_rawr::options::{ListingOptions, TimeFilter};
    /// use new_rawr::auth::AnonymousAuthenticator;
    /// let client = RedditClient::new("new_rawr", AnonymousAuthenticator::new());
    /// let sub = client.subreddit("thanksobama");
    /// let mut top = sub.top(ListingOptions::default(), TimeFilter::AllTime)
    ///     .expect("Request failed");
    /// assert_eq!(top.next().unwrap().title(), "Thanks Me");
    /// ```
    pub async fn top(&self, opts: ListingOptions, time: TimeFilter) -> Result<Listing<'_>, APIError> {
        let path = format!("top?{}&", time);
        self.get_feed(&path, opts).await
    }

    /// Gets a listing of the controversial feed for this subreddit. Also requires a time filter (
    /// `new_rawr::options::TimeFilter`) which is equivalent to the "links from: all time" dropdown
    /// on the website.
    pub async fn controversial(&self,
                         opts: ListingOptions,
                         time: TimeFilter)
                         -> Result<Listing<'_>, APIError> {
        let path = format!("controversial?{}&", time);
        self.get_feed(&path, opts).await
    }

    /// Submits a link post to this subreddit using the specified parameters. If the link has
    /// already been posted, this will fail unless you specifically allow reposts.
    /// # Examples
    /// ## Allowing a link to be reposted
    /// ```
    /// use new_rawr::options::LinkPost;
    /// let post = LinkPost::new("new_rawr!", "http://example.com").resubmit();
    /// ```
    /// ## Submitting a post
    /// ```rust,ignore
    /// use new_rawr::auth::PasswordAuthenticator;
    /// use new_rawr::client::RedditClient;
    /// use new_rawr::options::LinkPost;
    /// let client = RedditClient::new("new_rawr", PasswordAuthenticator::new("a", "b", "c", "d"));
    /// let sub = client.subreddit("rust");
    /// let post = LinkPost::new("new_rawr!", "http://example.com");
    /// sub.submit_link(post).expect("Posting failed!");
    /// ```
    pub async fn submit_link(&self, post: LinkPost) -> Result<(), APIError> {
        let body = format!("api_type=json&extension=json&kind=link&resubmit={}&sendreplies=true&\
                            sr={}&title={}&url={}",
                           post.resubmit,
                           self.name,
                           self.client.url_escape(post.title.to_owned()),
                           self.client.url_escape(post.link.to_owned()));
        self.client.post_success("/api/submit", &body, false).await
    }

    /// Submits a text post (self post) to this subreddit using the specified title and body.
    /// # Examples
    /// ## Submitting a post
    /// ```rust,ignore
    /// use new_rawr::auth::PasswordAuthenticator;
    /// use new_rawr::client::RedditClient;
    /// use new_rawr::options::SelfPost;
    /// let client = RedditClient::new("new_rawr", PasswordAuthenticator::new("a", "b", "c", "d"));
    /// let sub = client.subreddit("rust");
    /// let post = SelfPost::new("I love new_rawr!", "You should download it *right now*!");
    /// sub.submit_text(post).expect("Posting failed!");
    /// ```
    pub async fn submit_text(&self, post: SelfPost) -> Result<(), APIError> {
        let body = format!("api_type=json&extension=json&kind=self&sendreplies=true&sr={}\
                            &title={}&text={}",
                           self.name,
                           self.client.url_escape(post.title),
                           self.client.url_escape(post.text));
        self.client.post_success("/api/submit", &body, false).await
    }
    /// Invites a new member to the subreddit.
    pub async fn invite_member(&self, username: String) -> Result<bool, APIError> {
        let path = format!("/r/{}/api/friend", self.name);
        let body = format!("name={}&type=contributor", username);
        let result = self.client.post_json(&*path, &body, false).await;
        if result.is_err() {
            return Err(result.err().unwrap());
        }
        let value: Value = serde_json::from_str(&*result.unwrap()).unwrap();
        let x = value["success"].as_bool().unwrap();
        Ok(x)
    }

    /// Fetches information about a subreddit such as subscribers, active users and sidebar
    /// information.
    /// # Examples
    /// ```ignore
    /// use new_rawr::auth::AnonymousAuthenticator;
    /// use new_rawr::client::RedditClient;
    /// let client = RedditClient::new("new_rawr", AnonymousAuthenticator::new());
    /// let learn_programming = client.subreddit("learnprogramming").about()
    ///     .expect("Could not fetch 'about' data");
    /// assert_eq!(learn_programming.display_name(), "learnprogramming");
    /// ```
    pub async fn about(&self) -> Result<SubredditAbout, APIError> {
        let url = format!("/r/{}/about?raw_json=1", self.name);

        let string = self.client
            .get_json(&url, false).await.unwrap();
        let string: listing::SubredditAboutData = serde_json::from_str(&*string).unwrap();
        Ok(SubredditAbout::new(string))
    }
    ///  Get users
    pub async fn contributors(&self) -> Result<UserListing<'a>, APIError> {
        let url = format!("/r/{}/about/contributors?raw_json=1", self.name);
        let string = self.client
            .get_json(&url, false).await.unwrap();
        let json: Result<listing::UserListing, serde_json::Error> = serde_json::from_str(string.as_str());
        if json.is_err() {
            println!("{}", &json.err().unwrap().to_string());
            return Err(APIError::ExhaustedListing);
        } else {
            return Ok(UserListing::new(self.client, url, json.unwrap()));
        }
    }
    /// Subscribes to the specified subredit, returning the result to show whether the API call
    /// succeeded or not.
    pub async fn subscribe(&self) -> Result<(), APIError> {
        let body = format!("action=sub&sr_name={}", self.name);
        self.client.post_success("/api/subscribe", &body, false).await
    }

    /// Unsubscribes to the specified subreddit, returning the result to show whether the API call
    /// succeeded or not.
    pub async fn unsubscribe(&self) -> Result<(), APIError> {
        let body = format!("action=unsub&sr_name={}", self.name);
        self.client.post_success("/api/subscribe", &body, false).await
    }
}

/// Information about a subreddit such as subscribers, sidebar text and active users.
pub struct SubredditAbout {
    data: listing::SubredditAboutData,
}

impl Created for SubredditAbout {
    fn created(&self) -> i64 {
        self.data.created as i64
    }

    fn created_utc(&self) -> i64 {
        self.data.created_utc as i64
    }
}

impl SubredditAbout {
    /// Creates a new `SubredditAbout` instance. Use `Subreddit.about()` instead to get
    /// information about a subreddit.
    pub fn new(data: listing::SubredditAboutData) -> SubredditAbout {
        SubredditAbout { data: data }
    }

    /// The number of subscribers to this subreddit.
    pub fn subscribers(&self) -> u64 {
        self.data.subscribers
    }

    /// The number of logged-in users who have viewed this subreddit in the last 15
    /// minutes.
    pub fn active_users(&self) -> u64 {
        self.data.accounts_active
    }

    /// Returns `true` if the subreddit is visible to the public (i.e. not invitation only)
    pub fn public(&self) -> bool {
        self.data.public_traffic
    }

    /// The display name of the subreddit, not including leading /r/
    pub fn display_name(&self) -> &str {
        &self.data.display_name
    }
}
