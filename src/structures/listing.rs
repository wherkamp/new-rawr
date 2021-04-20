use std::vec::IntoIter;
use std::collections::VecDeque;
use std::thread;
use std::time::Duration;

use crate::responses::listing;
use crate::client::RedditClient;
use crate::structures::submission::Submission;
use crate::traits::{Content, PageListing};
use crate::errors::APIError;
use async_trait::async_trait;

/// A paginated listing of posts that can be iterated through. Posts are fetched lazily
/// until the listing is exhausted (similar to an infinite scroll of posts).
/// # Examples
/// ```rust,no_run
/// use new_rawr::client::RedditClient;
/// use new_rawr::options::ListingOptions;
/// use new_rawr::auth::AnonymousAuthenticator;
/// let client = RedditClient::new("new_rawr", AnonymousAuthenticator::new());
/// let sub = client.subreddit("redditdev");
/// let mut hot = sub.hot(ListingOptions::default()).expect("Could not get hot posts");
/// for post in hot.take(500) {
///     // Do something with each post here
/// }
/// ```
/// # Gotchas
/// Be careful when looping directly over a listing - if you're iterating through a very long
/// listing, like /r/all/new, your code never stop!
///
/// Instead, prefer to use `Listing.take(n)` if possible, or require user input before continuing
/// to page.
///
/// ## Improving Performance
/// By default, new_rawr paginates using the same `limit` parameter as you
/// (`ListingOptions::default()` sets it to 25), so by default you can only fetch 25 posts
/// at a time. Create a `ListingOptions` object with a batch size of 100 to reduce the amount of
/// requests that are needed, like this:
///
/// ```
/// # use new_rawr::options::ListingOptions;
/// use new_rawr::options::ListingAnchor;
/// ListingOptions {
///     batch: 100,
///     anchor: ListingAnchor::None
/// };
/// ```
///
/// Keep in mind that if you only want 5 or 10 items, you might save bandwidth and get a quicker
/// response by using a smaller batch size (and the Reddit admins would love it if you didn't
/// waste bandwidth!)
pub struct Listing<'a> {
    client: &'a RedditClient,
    query_stem: String,
    data: listing::ListingData<listing::SubmissionData>,
}

impl<'a> Listing<'a> {
    /// Internal method. Use other functions that return Listings, such as `Subreddit.hot()`.
    pub fn new(client: &RedditClient,
               query_stem: String,
               data: listing::ListingData<listing::SubmissionData>)
               -> Listing {
        Listing {
            client: client,
            query_stem: query_stem,
            data: data,
        }
    }
}

impl<'a> PageListing for Listing<'a> {
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

impl<'a> Listing<'a> {
    async fn fetch_after(&mut self) -> Result<Listing<'a>, APIError> {
        match self.after() {
            Some(after_id) => {
                let url = format!("{}&after={}", self.query_stem, after_id);
                let string = self.client
                    .get_json(&url, false).await.unwrap();
                let string :listing::Listing= serde_json::from_str(&*string).unwrap();
                Ok(Listing::new(self.client, self.query_stem.to_owned(), string.data))

            }
            None => Err(APIError::ExhaustedListing),
        }
    }
}
