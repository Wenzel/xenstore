use std::{cell::Cell, collections::HashMap};

use futures::{
    channel::oneshot,
    io::{self, Error, ErrorKind},
};
use log::{debug, info, warn};
use thiserror::Error;
use uuid::Uuid;

use crate::wire::{XsMessage, XsMessageType};

/// Maximum number of pending requests.
const MAX_REQUEST_COUNT: usize = 32;

#[derive(Error, Debug)]
pub enum XsAsyncError {
    #[error("no available slot")]
    NoSlot,
    #[error("unwatch without watch")]
    UnwatchNoWatch,
    #[error("channel error")]
    BrokenChannel(#[source] flume::SendError<XsMessage>),
    #[error("unwatch failed")]
    UnwatchFailure,
    #[error("received invalid response")]
    InvalidResponse { expected: XsMessageType },
    #[error("received invalid request id")]
    InvalidRequestId,
    #[error("no related request to this req_id")]
    UnknownRequestId,
    #[error("got non-uuid token")]
    NonUuidToken(#[source] uuid::Error),
    #[error("got non-utf8 payload")]
    NonUtf8Payload(#[source] std::str::Utf8Error),
    #[error("got invalid payload")]
    InvalidPayload,
}

impl From<flume::SendError<XsMessage>> for XsAsyncError {
    fn from(value: flume::SendError<XsMessage>) -> Self {
        Self::BrokenChannel(value)
    }
}

#[derive(Clone, Copy)]
pub struct XsWatchToken(Uuid);

pub struct XsAsyncRequest {
    pub request: XsMessage,
    pub response_sender: oneshot::Sender<XsMessage>,
}

pub enum XsAsyncMessage {
    Request(XsAsyncRequest),
    WatchSubscribe {
        path: Box<str>,
        event_sender: flume::Sender<Box<str>>,
        result_channel: oneshot::Sender<io::Result<XsWatchToken>>,
    },
    WatchUnsubscribe(XsWatchToken),
}

enum XsAsyncTask {
    Request(oneshot::Sender<XsMessage>),
    WatchSubscribe {
        subscriber_info: WatchSubscriberInfo,
        result_channel: oneshot::Sender<io::Result<XsWatchToken>>,
        token: XsWatchToken,
    },
    WatchUnsubscribe(XsWatchToken),
}

struct WatchSubscriberInfo {
    channel: flume::Sender<Box<str>>,
    // We need to store the watch patch as it is required by UNWATCH.
    path: Box<str>,
}

#[derive(Default)]
pub struct XsAsyncState {
    pending_tasks: [Cell<Option<XsAsyncTask>>; MAX_REQUEST_COUNT],
    watch_subscribers: HashMap<Uuid, WatchSubscriberInfo>,
    task_count: usize,
}

fn find_suitable_token<V>(watch_subscribers: &HashMap<Uuid, V>) -> XsWatchToken {
    // loop until there is no collision
    loop {
        let uuid = Uuid::new_v4();

        if watch_subscribers.get(&uuid).is_none() {
            return XsWatchToken(uuid);
        }
    }
}

impl XsAsyncState {
    async fn process_message(
        &mut self,
        message: XsAsyncMessage,
        xs_sender: &flume::Sender<XsMessage>,
    ) -> Result<(), XsAsyncError> {
        // Find a available task slot.
        let Some((req_id, slot)) = self
            .pending_tasks
            .iter_mut()
            .map(|slot| slot.get_mut())
            .enumerate()
            .find(|(_, slot)| slot.is_none())
        else {
            return Err(XsAsyncError::NoSlot);
        };

        match message {
            XsAsyncMessage::Request(XsAsyncRequest {
                mut request,
                response_sender,
            }) => {
                request.request_id = req_id as u32;

                xs_sender.send_async(request).await?;
                *slot = Some(XsAsyncTask::Request(response_sender));
                self.task_count += 1;
            }
            XsAsyncMessage::WatchSubscribe {
                path,
                event_sender: channel,
                result_channel,
            } => {
                let token = find_suitable_token(&self.watch_subscribers);

                // Make the actual WATCH command
                xs_sender
                    .send_async(XsMessage::from_string_slice(
                        XsMessageType::Watch,
                        req_id as u32,
                        &[&path, &token.0.to_string()],
                        true,
                    ))
                    .await?;

                // Wait until we get confirmation of the WATCH command by upstream.
                *slot = Some(XsAsyncTask::WatchSubscribe {
                    subscriber_info: WatchSubscriberInfo { channel, path },
                    result_channel,
                    token,
                });
                self.task_count += 1;
            }
            XsAsyncMessage::WatchUnsubscribe(token) => {
                let Some(WatchSubscriberInfo { path, .. }) = self.watch_subscribers.get(&token.0)
                else {
                    return Err(XsAsyncError::UnwatchNoWatch);
                };

                // Make the actual UNWATCH command
                xs_sender
                    .send_async(XsMessage::from_string_slice(
                        XsMessageType::Unwatch,
                        req_id as u32,
                        &[path, &token.0.to_string()],
                        true,
                    ))
                    .await?;

                // Wait until we got confirmation of the WATCH command by upstream.
                *slot = Some(XsAsyncTask::WatchUnsubscribe(token));
                self.task_count += 1;
            }
        }

        Ok(())
    }

    async fn process_response(&mut self, response: XsMessage) -> Result<(), XsAsyncError> {
        if response.msg_type == XsMessageType::WatchEvent {
            // Process a watch event (it's always req_id = 0) and is unsolicitated.
            return self.process_watch_entry(response).await;
        }

        // All other requests have a req_id and is solicitated,
        // thus they have a related pending_tasks entry.

        // Take a reference the the task slot (if any).
        let Some(slot) = self.pending_tasks.get_mut(response.request_id as usize) else {
            return Err(XsAsyncError::InvalidRequestId);
        };

        // Take it (leaving None at the place).
        let Some(task) = slot.take() else {
            return Err(XsAsyncError::UnknownRequestId);
        };
        self.task_count -= 1;

        match task {
            XsAsyncTask::Request(sender) => {
                // Usual request, forward response to caller (even if it is Error variant).
                sender.send(response).ok();
            }
            XsAsyncTask::WatchSubscribe {
                token,
                result_channel,
                subscriber_info,
            } => {
                match response.msg_type {
                    XsMessageType::Watch => {
                        // Now the watch is registered upstream, make it work here.
                        if self
                            .watch_subscribers
                            .insert(token.0, subscriber_info)
                            .is_some()
                        {
                            warn!("Overriden WATCH subscriber");
                        }

                        // Report that things got successful.
                        result_channel.send(Ok(token)).ok();
                    }
                    XsMessageType::Error => {
                        result_channel.send(Err(response.parse_error())).ok();
                    }
                    response => {
                        result_channel
                            .send(Err(Error::new(
                                ErrorKind::InvalidData,
                                format!("Got unexpected response ({response:?})"),
                            )))
                            .ok();
                        return Err(XsAsyncError::InvalidResponse {
                            expected: XsMessageType::Watch,
                        });
                    }
                }
            }
            XsAsyncTask::WatchUnsubscribe(token) => {
                match response.msg_type {
                    XsMessageType::Unwatch => {
                        // Make unwatch effective
                        if self.watch_subscribers.remove(&token.0).is_none() {
                            warn!("Unwatched nothing");
                        }
                    }
                    XsMessageType::Error => return Err(XsAsyncError::UnwatchFailure),
                    _ => {
                        return Err(XsAsyncError::InvalidResponse {
                            expected: XsMessageType::Watch,
                        })
                    }
                }
            }
        }

        Ok(())
    }

    async fn process_watch_entry(&mut self, message: XsMessage) -> Result<(), XsAsyncError> {
        let [value, token] = message
            .parse_payload_list()
            .map_err(XsAsyncError::NonUtf8Payload)?[..]
        else {
            return Err(XsAsyncError::InvalidPayload);
        };

        let uuid = Uuid::try_parse(token).map_err(XsAsyncError::NonUuidToken)?;

        if let Some(subscriber) = self.watch_subscribers.get_mut(&uuid) {
            if let Err(e) = subscriber.channel.send_async(value.into()).await {
                warn!("Lost watch subscriber: {e}");

                // Subscriber is dead, remove it.
                self.watch_subscribers.remove(&uuid);
            }
        } else {
            // Some kernels may misorder the watch event and watch response, try to
            // deal with these cases by preemptively sending the event to its related
            // channel.
            // We lookup in pending task if any task matches the watch_event, and
            // send the event to its dedicated channel, so we won't lose the event.
            if let Some(channel) = self
                .pending_tasks
                .iter_mut()
                .flat_map(Cell::get_mut)
                .find_map(|task| match task {
                    XsAsyncTask::WatchSubscribe {
                        token,
                        subscriber_info,
                        ..
                    } if token.0 == uuid => Some(subscriber_info.channel.clone()),
                    _ => None,
                })
            {
                debug!("Processing out of order watch response/event ({uuid})");
                channel.send(value.into()).ok();
            } else {
                warn!("Unregistered watch message ? ({uuid})");
            }
        }

        Ok(())
    }

    pub async fn run(
        mut self,
        message_channel: flume::Receiver<XsAsyncMessage>,
        xs_receiver: flume::Receiver<XsMessage>,
        xs_sender: flume::Sender<XsMessage>,
    ) {
        loop {
            if self.task_count == MAX_REQUEST_COUNT {
                // We can't process another task, only interface responses.
                debug!("Too much tasks");
                let Ok(response) = xs_receiver.recv_async().await else {
                    break;
                };

                if let Err(e) = self.process_response(response).await {
                    warn!("Process response failure: {e}")
                }
            } else {
                futures::select! {
                    response = xs_receiver.recv_async() => {
                        let Ok(response) = response else { break };
                        if let Err(e) = self.process_response(response).await {
                            warn!("Process response failure: {e}")
                        }
                    },
                    message = message_channel.recv_async() => {
                        let Ok(message) = message else { break };
                        if let Err(e) = self.process_message(message, &xs_sender).await {
                            warn!("Process message failure: {e}")
                        }
                    },
                    // In case we get a None, something is dead in the loop, stop here.
                    complete => break,
                }
            }
        }

        info!("Communication channel died");
    }
}
