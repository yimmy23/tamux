use super::*;
use tempfile::tempdir;
use tokio::time::{timeout, Duration};

mod part1;
mod part2;
mod part3;

async fn recv_until_qr(rx: &mut broadcast::Receiver<WhatsAppLinkEvent>) -> Option<String> {
    for _ in 0..8 {
        if let Ok(Ok(WhatsAppLinkEvent::Qr { ascii_qr, .. })) =
            timeout(Duration::from_millis(250), rx.recv()).await
        {
            return Some(ascii_qr);
        }
    }
    None
}

async fn recv_until_linked(
    rx: &mut broadcast::Receiver<WhatsAppLinkEvent>,
) -> Option<Option<String>> {
    for _ in 0..8 {
        if let Ok(Ok(WhatsAppLinkEvent::Linked { phone })) =
            timeout(Duration::from_millis(250), rx.recv()).await
        {
            return Some(phone);
        }
    }
    None
}

async fn recv_until_disconnected(
    rx: &mut broadcast::Receiver<WhatsAppLinkEvent>,
) -> Option<Option<String>> {
    for _ in 0..10 {
        if let Ok(Ok(WhatsAppLinkEvent::Disconnected { reason })) =
            timeout(Duration::from_millis(250), rx.recv()).await
        {
            return Some(reason);
        }
    }
    None
}

async fn recv_until_error(
    rx: &mut broadcast::Receiver<WhatsAppLinkEvent>,
) -> Option<(String, bool)> {
    for _ in 0..10 {
        if let Ok(Ok(WhatsAppLinkEvent::Error {
            message,
            recoverable,
        })) = timeout(Duration::from_millis(250), rx.recv()).await
        {
            return Some((message, recoverable));
        }
    }
    None
}
