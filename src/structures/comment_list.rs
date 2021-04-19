use std::vec::IntoIter;
use std::collections::VecDeque;
use std::thread;
use std::time::Duration;

use std::collections::HashMap;
use crate::client::RedditClient;
use crate::structures::comment::Comment;
use crate::responses::BasicThing;
use crate::responses::listing;
use crate::responses::comment::{CommentData, MoreData};
use serde_json::{Value, from_value, from_str};
use std::io::Read;
use crate::errors::APIError;
use crate::traits::Content;
use hyper::Body;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use std::task::{Poll, Context};
use std::pin::Pin;

/// A list of comments that can be iterated through. Automatically fetches 'more' links when
/// necessary until all comments have been consumed, which can lead to pauses while loading
/// from the API.
/// # Examples
/// ```
/// use new_rawr::client::RedditClient;
/// use new_rawr::options::ListingOptions;
/// use new_rawr::traits::Commentable;
/// use new_rawr::auth::AnonymousAuthenticator;
/// let client = RedditClient::new("new_rawr", AnonymousAuthenticator::new());
/// let announcements = client.subreddit("announcements");
/// let announcement = announcements.hot(ListingOptions::default())
///     .expect("Could not fetch announcements")
///     .next().unwrap();
/// // Usually less than 100 top-level comments are fetched at a time, but the CommentList
/// // fetches it for us!
/// let comments = announcement.replies().expect("Could not get comments").take(100);
/// ```
pub struct CommentList<'a> {
    client: &'a RedditClient,
    comments: Vec<Comment<'a>>,
    comment_hashes: HashMap<String, usize>,
    more: Vec<MoreData>,
    link_id: String,
    parent: String,
}

impl<'a> CommentList<'a> {
    /// Creates a `CommentList` by storing all comments in the `CommentList.comments` list
    /// and all 'more' items in the `CommentList.more` list. Do not use this method - instead, use
    /// `Submission.replies()` or `Comment.replies()`.
    pub fn new(client: &'a RedditClient,
               link_id: String,
               parent: String,
               comment_list: Vec<BasicThing<Value>>)
               -> CommentList<'a> {
        let mut new_items = vec![];
        let mut new_mores = vec![];
        let mut hashes = HashMap::new();
        for item in comment_list {
            if item.kind == "t1" {
                let item = from_value::<CommentData>(item.data).unwrap();
                let comment = Comment::new(client, item);
                hashes.insert(comment.name().to_owned(), new_items.len());
                new_items.push(comment);
            } else if item.kind == "more" {
                let item = from_value::<MoreData>(item.data).unwrap();
                new_mores.push(item);
            } else {
                unreachable!();
            }
        }

        CommentList {
            client: client,
            comments: new_items,
            more: new_mores,
            comment_hashes: hashes,
            link_id: link_id,
            parent: parent,
        }
    }

    /// Creates an empty listing, when there are no comments to show.
    pub fn empty(client: &'a RedditClient) -> CommentList<'a> {
        CommentList {
            client: client,
            link_id: String::new(),
            parent: String::new(),
            comments: Vec::new(),
            more: Vec::new(),
            comment_hashes: HashMap::new(),
        }
    }

    /// Adds a (pre-existing) comment to the reply list. This is an internal method, and does not
    /// actually post a comment, just adds one that has already been fetched.
    pub fn add_reply(&mut self, item: Comment<'a>) {
        self.comment_hashes.insert(item.name().to_owned(), self.comments.len());
        self.comments.push(item);
    }

    async fn fetch_more(&mut self, more_item: MoreData) -> Result<CommentList<'a>, APIError> {
        let params = format!("api_type=json&raw_json=1&link_id={}&children={}",
                             &self.link_id,
                             &more_item.children.join(","));
        let url = "/api/morechildren";
        self.client.ensure_authenticated();
        let request = self.client.post(url, false).body(Body::from(params.clone())).unwrap();


        let res = self.client.client.request(request).await.unwrap();
        if res.status().is_success() {
            // The "data" attribute is sometimes not present, so we have to unwrap it all
            // manually
            let value = hyper::body::to_bytes(res.into_body()).await;

            let value = String::from_utf8(value.unwrap().to_vec());

            let mut new_listing: Value = from_str(value.unwrap().as_str()).unwrap();
            let new_listing = new_listing.as_object_mut().unwrap();
            let mut json = new_listing.remove("json").unwrap();
            let json = json.as_object_mut().unwrap();
            let data = json.remove("data");
            if let Some(mut data) = data {
                let things = data.as_object_mut().unwrap();
                let things = things.remove("things").unwrap();
                let things: Vec<BasicThing<Value>> = from_value(things).unwrap();
                Ok(CommentList::new(self.client,
                                    self.link_id.to_owned(),
                                    self.parent.to_owned(),
                                    things))
            } else {
                Ok(CommentList::new(self.client,
                                    self.link_id.to_owned(),
                                    self.parent.to_owned(),
                                    vec![]))
            }
        } else {
            Err(APIError::HTTPError(res.status()))
        }
    }

    fn merge_more_comments(&mut self, list: CommentList<'a>) {
        let mut orphans: HashMap<String, Vec<Comment>> = HashMap::new();
        for item in list.comments {
            self.merge_comment(item, &mut orphans);
        }
    }
    async fn next_comment(&mut self) -> Option<Comment<'a>> {
        if self.comments.is_empty() {
            if self.more.is_empty() {
                None
            } else {
                // XXX: This code is hideous (see the fetch_more etc.) but it does work.
                // TODO: refactor (carefully!)
                let more_item = self.more.drain(..1).next().unwrap();
                let mut new_listing = self.fetch_more(more_item).await.unwrap();
                self.more.append(&mut new_listing.more);
                // We've already consumed all of the items, so we can remove the mapping now.
                self.comment_hashes = HashMap::new();
                self.merge_more_comments(new_listing);
                return self.next_comment().await
            }
        } else {
            // Draining breaks the comment_hashes map!
            let child = self.comments.drain(..1).next().unwrap();
            Some(child)
        }
    }

    fn merge_comment(&mut self,
                     mut item: Comment<'a>,
                     mut orphanage: &mut HashMap<String, Vec<Comment<'a>>>) {
        {
            if item.parent() == self.parent {
                self.add_reply(item);
                return;
            }

            let parent = self.comment_hashes.get(item.parent());
            if let Some(pos) = parent {
                self.comments[*pos].add_reply(item);
                return;
            }
        }
        {
            if let Some(orphaned) = orphanage.remove(item.parent()) {
                // The orphaned children will now be added to their parent.
                for orphan in orphaned {
                    item.add_reply(orphan);
                }
                self.merge_comment(item, &mut orphanage);
            } else {
                let name = item.name().to_owned();
                if let Some(mut list) = orphanage.remove(&name) {
                    list.push(item);
                    orphanage.insert(name, list);
                } else {
                    orphanage.insert(name, vec![item]);
                }
            }
        }
    }
}

