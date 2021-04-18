use crate::structures::submission::FlairList;
use crate::structures::listing::Listing;
use crate::client::RedditClient;
use crate::responses::{FlairSelectorResponse, listing};
use crate::responses::user::{UserAbout as _UserAbout, UserAboutData, UserAboutDataCore};
use crate::responses::listing::{Listing as _Listing, UserListingData};
use crate::traits::{Created, PageListing};
use crate::errors::APIError;
use crate::structures::comment_list::CommentList;
use crate::responses::comment::CommentListing;
use std::error::Error;

/// Interface to a Reddit user, which can be used to access their karma and moderator status.
pub struct User<'a> {
    client: &'a RedditClient,
    /// The name of the user that this struct represents.
    pub name: String,
}

impl<'a> User<'a> {
    /// Internal method. Use `RedditClient.user(NAME)` instead.
    pub fn new(client: &'a RedditClient, name: &str) -> User<'a> {
        User {
            client: client,
            name: name.to_owned(),
        }
    }
    /// Gets information about this user.
    /// # Example
    /// ```
    /// use new_rawr::client::RedditClient;
    /// use new_rawr::auth::AnonymousAuthenticator;
    /// let client = RedditClient::new("new_rawr", AnonymousAuthenticator::new());
    /// let user = client.user("Aurora0001").about().expect("User request failed");
    /// assert_eq!(user.id(), "eqyvc");
    /// ```
    pub fn about(self) -> Result<UserAbout, APIError> {
        UserAbout::new(self.client, self.name)
    }

    /// Gets a list of possible **user** flairs that can be added in this subreddit.
    ///
    /// User flairs apply on a per-subreddit basis, and some may not permit user flairs at all.
    /// If you do not have the privileges to set the flair for this user, you will receive
    /// a 403 error.
    pub fn flair_options(&self, subreddit: &str) -> Result<FlairList, APIError> {
        let body = format!("user={}", self.name);
        let url = format!("/r/{}/api/flairselector", subreddit);
        let string = self.client
            .post_json(&url, &body, false).unwrap();
        let string: FlairSelectorResponse = serde_json::from_str(&*string).unwrap();
        Ok(FlairList::new(string.choices))
    }

    /// Sets the flair for this user in the specified subreddit, using the specified template
    /// string. You can get the template string from `flair_options`; either:
    /// - use the returned `FlairList` and call the method `find_text` which will return the
    /// template ID of the flair with the specified text.
    /// - iterate through the `FlairList`, and get the `FlairChoice.flair_template_id` value.
    pub fn flair(&self, subreddit: &str, template: &str) -> Result<(), APIError> {
        let body = format!("api_type=json&user={}&flair_template_id={}",
                           self.name,
                           template);
        let url = format!("/r/{}/api/selectflair", subreddit);
        self.client.post_success(&url, &body, false)
    }

    /// Gets a list of *submissions* that the specified user has submitted. This endpoint is a
    /// listing and will continue yielding items until every item has been exhausted.
    /// # Examples
    /// ```
    ///
    /// use new_rawr::client::RedditClient;
    /// use new_rawr::auth::AnonymousAuthenticator;
    /// let client = RedditClient::new("new_rawr", AnonymousAuthenticator::new());
    /// let user = client.user("Aurora0001");
    /// let submissions = user.submissions().expect("Could not fetch!");
    /// let mut i = 0;
    /// for submission in submissions.take(5) {
    ///     i += 1;
    /// }
    /// assert_eq!(i, 5);
    /// ```
    pub fn submissions(&self) -> Result<Listing, APIError> {
        let url = format!("/user/{}/submitted?raw_json=1", self.name);
        let result = self.client
            .get_json(&url, false).unwrap();
        let result: _Listing = serde_json::from_str(&*result).unwrap();
        Ok(Listing::new(self.client, url, result.data))
    }
    // TODO: implement comment, overview, gilded listings etc.
    ///Incomplete get comments
    pub fn comments(&self) -> Result<CommentListing, APIError> {
        let url = format!("/user/{}/comments?raw_json=1", self.name);
        let result = self.client
            .get_json(&url, false).unwrap();
        let result: CommentListing = serde_json::from_str(&*result).unwrap();
        //TODO make structure for Comments
        Ok(result)
    }
}

/// Information about a user from /r/username/about, such as karma and ID.
pub struct UserAbout {
    ///About data for the user
    pub data: UserAboutData,
}

impl UserAbout {
    /// Internal method. Use `RedditClient.user(NAME).about()` instead.
    pub fn new(client: &RedditClient, name: String) -> Result<UserAbout, APIError> {
        let url = format!("/user/{}/about?raw_json=1", name);
        let result = client.get_json(&url, false).unwrap();
        let result: Result<UserAboutDataCore, serde_json::Error> = serde_json::from_str(&*result);
        if result.is_err(){
            return Err(APIError::JSONError(result.err().unwrap()));
        }
        Ok(UserAbout {
            data: result.unwrap().data
        })
    }

    /// Gets the user's link karma (including self post karma as of July 19th, 2016).
    pub fn link_karma(&self) -> i64 {
        self.data.link_karma
    }

    /// Gets the user's comment karma.
    pub fn comment_karma(&self) -> i64 {
        self.data.comment_karma
    }

    /// Gets the user ID, not including kind, e.g. 'eqyvc'.
    pub fn id(&self) -> &str {
        &self.data.id
    }
}

impl Created for UserAbout {
    fn created(&self) -> i64 {
        self.data.created as i64
    }

    fn created_utc(&self) -> i64 {
        self.data.created_utc as i64
    }
}


pub struct UserListing<'a> {
    client: &'a RedditClient,
    query_stem: String,
    data: listing::UserListing,
}

impl<'a> UserListing<'a> {
    /// Internal method. Use other functions that return Listings, such as `Subreddit.hot()`.
    pub fn new(client: &RedditClient,
               query_stem: String,
               data: listing::UserListing)
               -> UserListing {
        UserListing {
            client: client,
            query_stem: query_stem,
            data: data,
        }
    }
}

impl<'a> PageListing for UserListing<'a> {
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

impl<'a> UserListing<'a> {
    fn fetch_after(&mut self) -> Result<UserListing<'a>, APIError> {
        match self.after() {
            Some(after_id) => {
                let url = format!("{}&after={}", self.query_stem, after_id);
                let string = self.client
                    .get_json(&url, false).unwrap();
                let string: listing::UserListing = serde_json::from_str(&*string).unwrap();
                Ok(UserListing::new(self.client, self.query_stem.to_owned(), string))
            }
            None => Err(APIError::ExhaustedListing),
        }
    }
}

impl<'a> Iterator for UserListing<'a> {
    type Item = User<'a>;
    fn next(&mut self) -> Option<User<'a>> {
        if self.data.children.is_empty() {
            if self.after().is_none() {
                None
            } else {
                let mut new_listing = self.fetch_after().expect("After does not exist!");
                self.data.children.append(&mut new_listing.data.children);
                self.data.after = new_listing.data.after;
                self.next()
            }
        } else {
            let child = self.data.children.drain(..1).next().unwrap();
            Some(User::new(self.client, child.name.as_str()))
        }
    }
}
