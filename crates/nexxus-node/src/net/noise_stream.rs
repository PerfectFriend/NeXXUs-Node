//! Wire v4 transport adapter (ADR-0011): AsyncRead/AsyncWrite поверх Noise transport.
//! Кадр = u32 BE length || NoiseTransport::encrypt(payload).
//! Доступно только с feature `wire-v4`.

use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

use nexxus_core::wire::noise::NoiseTransport;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

const MAX_NOISE_FRAME: usize = 16 * 1024 * 1024;

enum RState {
    Len([u8; 4], usize),
    Body(Vec<u8>, usize),
}

pub struct NoiseStream<S> {
    inner: S,
    ts: NoiseTransport,
    rbuf: VecDeque<u8>,
    rstate: RState,
}

impl<S> NoiseStream<S> {
    pub fn new(inner: S, ts: NoiseTransport) -> Self {
        Self {
            inner,
            ts,
            rbuf: VecDeque::new(),
            rstate: RState::Len([0u8; 4], 0),
        }
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for NoiseStream<S> {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        loop {
            if !self.rbuf.is_empty() {
                let n = buf.remaining().min(self.rbuf.len());
                for _ in 0..n {
                    if let Some(b) = self.rbuf.pop_front() {
                        buf.put_slice(&[b]);
                    }
                }
                return Poll::Ready(Ok(()));
            }
            let mut st = std::mem::replace(&mut self.rstate, RState::Len([0u8; 4], 0));
            let mut early: Option<Poll<std::io::Result<()>>> = None;
            match &mut st {
                RState::Len(lenb, filled) => {
                    let mut rb = ReadBuf::new(&mut lenb[*filled..]);
                    match Pin::new(&mut self.inner).poll_read(cx, &mut rb) {
                        Poll::Ready(Ok(())) => {
                            let got = rb.filled().len();
                            if got == 0 {
                                early = Some(Poll::Ready(Ok(()))); // EOF
                            } else {
                                *filled += got;
                                if *filled == 4 {
                                    let n = u32::from_be_bytes(*lenb) as usize;
                                    if n == 0 || n > MAX_NOISE_FRAME {
                                        self.rstate = st;
                                        return Poll::Ready(Err(std::io::Error::new(
                                            std::io::ErrorKind::InvalidData,
                                            "bad noise frame length",
                                        )));
                                    }
                                    st = RState::Body(vec![0u8; n], 0);
                                }
                            }
                        }
                        other => early = Some(other),
                    }
                }
                RState::Body(ct, filled) => {
                    let mut rb = ReadBuf::new(&mut ct[*filled..]);
                    match Pin::new(&mut self.inner).poll_read(cx, &mut rb) {
                        Poll::Ready(Ok(())) => {
                            let got = rb.filled().len();
                            if got == 0 {
                                self.rstate = st;
                                return Poll::Ready(Err(std::io::Error::new(
                                    std::io::ErrorKind::UnexpectedEof,
                                    "short noise frame",
                                )));
                            }
                            *filled += got;
                            if *filled == ct.len() {
                                let ct = std::mem::take(ct);
                                match self.ts.decrypt(&ct) {
                                    Ok(pt) => {
                                        self.rbuf.extend(pt);
                                        st = RState::Len([0u8; 4], 0);
                                    }
                                    Err(e) => {
                                        self.rstate = st;
                                        return Poll::Ready(Err(std::io::Error::new(
                                            std::io::ErrorKind::InvalidData,
                                            e,
                                        )));
                                    }
                                }
                            }
                        }
                        other => early = Some(other),
                    }
                }
            }
            self.rstate = st;
            if let Some(p) = early {
                return p;
            }
        }
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for NoiseStream<S> {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, data: &[u8]) -> Poll<std::io::Result<usize>> {
        let ct = self
            .ts
            .encrypt(data)
            .map_err(std::io::Error::other)?;
        let mut frame = Vec::with_capacity(4 + ct.len());
        frame.extend_from_slice(&(ct.len() as u32).to_be_bytes());
        frame.extend_from_slice(&ct);
        match Pin::new(&mut self.inner).poll_write(cx, &frame) {
            Poll::Ready(Ok(_)) => Poll::Ready(Ok(data.len())),
            other => other,
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
