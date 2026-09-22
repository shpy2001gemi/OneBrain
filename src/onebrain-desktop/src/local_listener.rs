//! Cancellation reaches accepted HTTP and upgraded WS sockets, not just accept().
use std::{
    future::Future,
    io,
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream},
};
use tokio_util::sync::CancellationToken;

pub struct LocalListener {
    pub listener: TcpListener,
    pub cancel: CancellationToken,
}
pub struct LocalIo {
    stream: TcpStream,
    stopped: Pin<Box<dyn Future<Output = ()> + Send>>,
}
impl axum::serve::Listener for LocalListener {
    type Io = LocalIo;
    type Addr = SocketAddr;
    async fn accept(&mut self) -> (LocalIo, SocketAddr) {
        loop {
            match self.listener.accept().await {
                Ok((stream, address)) => {
                    return (
                        LocalIo {
                            stream,
                            stopped: Box::pin(self.cancel.clone().cancelled_owned()),
                        },
                        address,
                    )
                }
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(100)).await,
            }
        }
    }
    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }
}
impl AsyncRead for LocalIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.stopped.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}
impl AsyncWrite for LocalIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.stopped.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()));
        }
        Pin::new(&mut self.stream).poll_write(cx, bytes)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}
