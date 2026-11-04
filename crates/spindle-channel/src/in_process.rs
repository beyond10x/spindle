//! A channel inside one process: two queues, one per direction.

use std::sync::mpsc;
use std::time::Duration;

use crate::channel::{Channel, ChannelError};
use crate::model::Frame;

/// One end of an in-process link.
pub struct InProcess {
    outgoing: mpsc::Sender<Frame>,
    incoming: mpsc::Receiver<Frame>,
}

/// Two connected ends: what one sends, the other receives.
pub fn pair() -> (InProcess, InProcess) {
    let (to_far, at_far) = mpsc::channel();
    let (to_near, at_near) = mpsc::channel();
    (
        InProcess {
            outgoing: to_far,
            incoming: at_near,
        },
        InProcess {
            outgoing: to_near,
            incoming: at_far,
        },
    )
}

impl Channel for InProcess {
    fn send(&mut self, frame: Frame) -> Result<(), ChannelError> {
        self.outgoing.send(frame).map_err(|_| ChannelError::Closed)
    }

    fn receive(&mut self, wait: Duration) -> Result<Option<Frame>, ChannelError> {
        if wait.is_zero() {
            return match self.incoming.try_recv() {
                Ok(frame) => Ok(Some(frame)),
                Err(mpsc::TryRecvError::Empty) => Ok(None),
                Err(mpsc::TryRecvError::Disconnected) => Err(ChannelError::Closed),
            };
        }
        match self.incoming.recv_timeout(wait) {
            Ok(frame) => Ok(Some(frame)),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(ChannelError::Closed),
        }
    }
}
