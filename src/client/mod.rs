//! A client that represents one connection to the Reddit API. This can log in to one account
//! or remain anonymous, and performs all interactions with the Reddit API.
//! # Examples
//! ## Creating a RedditClient
//! When creating a `RedditClient`, you are only required to pass in a user agent string, which will
//! identify your client. The user agent should identify your program, but does not need to
//! be unique to this particular machine - you should use one user agent for each version of
//! your program. You **must** use a descriptive user agent when creating the client to comply
//! with Reddit API rules.
//!
//! The recommended format for user agent strings is `platform:program:version (by /u/yourname)`,
//! e.g. `linux:new_rawr:v0.0.1 (by /u/Aurora0001)`.
//!
//! You also need to pass in an *Authenticator*. `new_rawr` provides multiple authenticators that
//! use the different authentication flows provided by Reddit. To get started, you may just want
//! to browse anonymously. For this, `AnonymousAuthenticator` is provided, which can browse
//! reddit without any IDs or credentials.
//!
//! If you need logged-in privileges, you need to choose a different authenticator. For most
//! purposes, the appropriate authenticator will be `PasswordAuthenticator`. See the `auth` module
//! for examples of usage and benefits of this.
//!
//! ```
//! use new_rawr::client::RedditClient;
//! use new_rawr::auth::AnonymousAuthenticator;
//! let agent = "linux:new_rawr:v0.0.1 (by /u/Aurora0001)";
//! let client = RedditClient::new(agent, AnonymousAuthenticator::new());
//! ```

use std::borrow::Borrow;
use std::error::Error;
use std::io::Read;
use std::panic::resume_unwind;
use std::str::FromStr;
use std::sync::{Arc, Mutex, MutexGuard};

use futures::AsyncReadExt;
use hyper::{Body, Method, Request, StatusCode};
use hyper::client::{Client, HttpConnector};
use hyper::header::USER_AGENT;
use hyper::http::request::Builder;
use hyper::Uri;
use hyper_tls::HttpsConnector;
use serde::Deserialize;
use serde_json::from_str;

use crate::auth::Authenticator;
use crate::errors::APIError;
use crate::structures::messages::MessageInterface;
use crate::structures::submission::LazySubmission;
use crate::structures::subreddit::Subreddit;
use crate::structures::user::User;
use hyper::body::Buf;

/// A client to connect to Reddit. See the module-level documentation for examples.
pub struct RedditClient {
    /// The internal HTTP client. You should not need to manually use this. If you do, file an
    /// issue saying why the API does not support your use-case, and we'll try to add it.
    pub client: Client<HttpsConnector<HttpConnector>>,
    user_agent: String,
    authenticator: Arc<Mutex<Box<dyn Authenticator + Send>>>,
    auto_logout: bool,
}


impl RedditClient {
    /// Creates an instance of the `RedditClient` using the provided user agent.
    pub async fn new(user_agent: &str,
                     authenticator: Arc<Mutex<Box<dyn Authenticator + Send>>>)
                     -> RedditClient {
        // Connection pooling is problematic if there are pauses/sleeps in the program, so we
        // choose to disable it by using a non-pooling connector.
        let https = HttpsConnector::new();
        let client = Client::builder().build::<_, hyper::Body>(https);
        let this = RedditClient {
            client: client,
            user_agent: user_agent.to_owned(),
            authenticator: authenticator,
            auto_logout: true,
        };

        this.get_authenticator()
            .login(&this.client, &this.user_agent).await
            .expect("Authentication failed. Did you use the correct username/password?");
        this
    }

    /// Disables the automatic logout that occurs when the client drops out of scope.
    /// In the case of OAuth, it will prevent your access token or refresh token from being
    /// revoked, though they may expire anyway.
    ///
    /// Although not necessary, it is good practice to revoke tokens when you're done with them.
    /// This will **not** affect the client ID or client secret.
    /// # Examples
    /// ```rust,no_run
    /// use new_rawr::client::RedditClient;
    /// use new_rawr::auth::PasswordAuthenticator;
    /// let mut client = RedditClient::new("new_rawr", PasswordAuthenticator::new("a", "b", "c", "d"));
    /// client.set_auto_logout(false); // Auto-logout disabled. Set to `true` to enable.
    /// ```
    pub fn set_auto_logout(&mut self, val: bool) {
        self.auto_logout = val;
    }

    /// Checks if the time is over refresh.
    /// If the token was revoked for another reason an error will be thrown in the code later.
    pub async fn ensure_authenticated(&self) {
        if self.get_authenticator().needs_token_refresh() {
            let mut guard = self.get_authenticator();
            guard.refresh_token(&self.client, &*self.user_agent);
        }
    }

    /// Gets a mutable reference to the authenticator using a `&RedditClient`. Mainly used
    /// in the `ensure_authenticated` method to update tokens if necessary.
    pub fn get_authenticator(&self) -> MutexGuard<Box<Authenticator + Send + 'static>> {
        self.authenticator.lock().unwrap()
    }

    /// Provides an interface to the specified subreddit which can be used to access
    /// subreddit-related API endpoints such as post listings.
    pub fn subreddit(&self, name: &str) -> Subreddit {
        Subreddit::create_new(self, &self.url_escape(name.to_owned()))
    }

    /// Gets the specified user in order to get user-related data such as the 'about' page.
    pub fn user(&self, name: &str) -> User {
        User::new(self, &self.url_escape(name.to_owned()))
    }

    /// Creates a full URL using the correct access point (API or OAuth) from the stem.
    pub fn build_url(&self,
                     dest: &str,
                     oauth_required: bool,
                     authenticator: &mut MutexGuard<Box<Authenticator + Send + 'static>>)
                     -> String {
        let oauth_supported = authenticator.oauth();
        let stem = if oauth_required || oauth_supported {
            // All endpoints support OAuth, but some do not support the regular endpoint. If we are
            // required to use it or support it, we will use it.
            assert!(oauth_supported,
                    "OAuth is required to use this endpoint, but your authenticator does not \
                     support it.");
            "https://oauth.reddit.com"
        } else {
            "https://api.reddit.com"
        };
        format!("{}{}", stem, dest)
    }

    /// Wrapper around the `get` function of `hyper::client::Client`, which sends a HTTP GET
    /// request. The correct user agent header is also sent using this function, which is necessary
    /// to prevent 403 errors.
    pub fn get(&self, dest: &str, oauth_required: bool) -> Builder {
        let mut authenticator = self.get_authenticator();
        let url = self.build_url(dest, oauth_required, &mut authenticator);

        let mut builder = (Builder::new());
        let mut headers = authenticator.headers();

        for x in headers {
            builder = builder.header(x.0, x.1);
        }
        builder.method(Method::GET).uri(url).header(USER_AGENT, self.user_agent.to_owned())
    }

    /// Sends a GET request with the specified parameters, and returns the resulting
    /// deserialized object.
    pub async fn get_json(&self, dest: &str, oauth_required: bool) -> Result<String, APIError> {
        self.ensure_authenticated().await;
        let request = self.get(dest, oauth_required).body(Body::empty()).unwrap();


        let response = self.client.request(request).await.unwrap();
        if response.status().is_success() {
            let value = hyper::body::to_bytes(response.into_body()).await;
            Ok(String::from_utf8(value.unwrap().to_vec()).unwrap().parse().unwrap())
        } else {
            Err(APIError::HTTPError(response.status()))
        }
    }

    /// Wrapper around the `post` function of `hyper::client::Client`, which sends a HTTP POST
    /// request. The correct user agent header is also sent using this function, which is necessary
    /// to prevent 403 errors.
    pub fn post(&self, dest: &str, oauth_required: bool) -> Builder {
        let mut authenticator = self.get_authenticator();
        let url = self.build_url(dest, oauth_required, &mut authenticator);
        let mut builder = Request::builder().method(Method::POST).uri(url);
        let mut headers = authenticator.headers();

        for x in headers {
            builder = builder.header(x.0, x.1);
        }
        builder.header(USER_AGENT, self.user_agent.to_owned())
    }

    /// Sends a post request with the specified parameters, and converts the resulting JSON
    /// into a deserialized object.
    pub async fn post_json(&self, dest: &str, body: &str, oauth_required: bool) -> Result<String, APIError> {
        self.ensure_authenticated().await;
        let request = self.post(dest, oauth_required).body(Body::from(body.to_string())).unwrap();


        let response = self.client.request(request).await.unwrap();
        let status = response.status();
        if status.is_success() {
            let value = hyper::body::to_bytes(response.into_body()).await;
            Ok(String::from_utf8(value.unwrap().to_vec()).unwrap().parse().unwrap())
        } else {
            Err(APIError::HTTPError(status))
        }
    }

    /// Sends a post request with the specified parameters, and ensures that the response
    /// has a success header (HTTP 2xx).
    pub async fn post_success(&self,
                              dest: &str,
                              body: &str,
                              oauth_required: bool)
                              -> Result<(), APIError> {
        self.ensure_authenticated().await;
        let request = self.post(dest, oauth_required).body(Body::from(body.to_string())).unwrap();

        let runtime = tokio::runtime::Runtime::new().expect("Unable to create a runtime");

        let response = runtime.block_on(self.client.request(request)).unwrap();
        if response.status().is_success() {
            Ok(())
        } else {
            Err(APIError::HTTPError(response.status()))
        }
    }

    /// URL encodes the specified string so that it can be sent in GET and POST requests.
    ///
    /// This is only done when data is being sent that isn't from the API (we assume that API
    /// data is safe)
    /// # Examples
    /// ```
    /// # use new_rawr::client::RedditClient;
    /// # use new_rawr::auth::AnonymousAuthenticator;
    /// # let client = RedditClient::new("new_rawr", AnonymousAuthenticator::new());
    /// assert_eq!(client.url_escape(String::from("test&co")), String::from("test%26co"));
    /// assert_eq!(client.url_escape(String::from("👍")), String::from("%F0%9F%91%8D"));
    /// assert_eq!(client.url_escape(String::from("\n")), String::from("%0A"))
    /// ```
    pub fn url_escape(&self, item: String) -> String {
        let mut res = String::new();
        for character in item.chars() {
            match character {
                ' ' => res.push('+'),
                '*' | '-' | '.' | '0'...'9' | 'A'...'Z' | '_' | 'a'...'z' => res.push(character),
                _ => {
                    for val in character.to_string().as_bytes() {
                        res = res + &format!("%{:02X}", val);
                    }
                }
            }
        }
        res
    }

    /// Gets a `LazySubmission` object which can be used to access the information/comments of a
    /// specified post. The **full** name of the item should be used.
    /// # Examples
    /// ```
    ///
    /// use new_rawr::client::RedditClient;
    /// use new_rawr::auth::AnonymousAuthenticator;
    /// let client = RedditClient::new("new_rawr", AnonymousAuthenticator::new());
    /// let post = client.get_by_id("t3_4uule8").get().expect("Could not get post.");
    /// assert_eq!(post.title(), "[C#] Abstract vs Interface");
    /// ```
    pub fn get_by_id(&self, id: &str) -> LazySubmission {
        LazySubmission::new(self, &self.url_escape(id.to_owned()))
    }

    /// Gets a `MessageInterface` object which allows access to the message listings (e.g. `inbox`,
    /// `unread`, etc.)
    /// # Examples
    /// ```rust,no_run
    ///
    /// use new_rawr::auth::PasswordAuthenticator;
    /// use new_rawr::client::RedditClient;
    /// use new_rawr::options::ListingOptions;
    /// let client = RedditClient::new("new_rawr", PasswordAuthenticator::new("a", "b", "c", "d"));
    /// let messages = client.messages();
    /// for message in messages.unread(ListingOptions::default()) {
    ///
    /// }
    /// ```
    pub fn messages(&self) -> MessageInterface {
        MessageInterface::new(self)
    }
}

impl Drop for RedditClient {
    fn drop(&mut self) {
        if self.auto_logout {
            let runtime = tokio::runtime::Runtime::new().expect("Unable to create a runtime");

            let result = runtime.block_on(self.get_authenticator().logout(&self.client, &self.user_agent));
        }
    }
}
