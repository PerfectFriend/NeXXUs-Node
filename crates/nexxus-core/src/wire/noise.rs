//! Wire v4 transport handshake — Noise_XX_25519_ChaChaPoly_SHA256 (ADR-0011, закрывает H-4).
//!
//! Prologue = H("nexxus-wire-v4" ‖ node_id_self ‖ node_id_peer ‖ epoch) — биндит сессию
//! к обеим идентичностям и эпохе (anti-replay/relay между узлами).
//! После handshake — эфемерные сессионные ключи (forward secrecy), долгосрочные
//! identity-ключи участвуют только в самом handshake.
//!
//! Модуль доступен только с feature `wire-v4`.

use sha2::{Digest, Sha256};
use snow::{Builder, HandshakeState, TransportState};

use crate::types::NodeId;

pub const WIRE_V4_PROLOGUE_TAG: &[u8] = b"nexxus-wire-v4";
pub const MAX_NOISE_MSG: usize = 65535;

/// Prologue для handshake: бинд к self/peer/epoch.
pub fn prologue(self_id: &NodeId, peer_id: &NodeId, epoch: u64) -> Vec<u8> {
    // Prologue одинаков на обеих сторонах: канонический порядок id (anti-relay между узлами).
    let (a, b) = if self_id.0 <= peer_id.0 { (self_id.0, peer_id.0) } else { (peer_id.0, self_id.0) };
    let mut h = Sha256::new();
    h.update(WIRE_V4_PROLOGUE_TAG);
    h.update(a);
    h.update(b);
    h.update(epoch.to_be_bytes());
    h.finalize().to_vec()
}

fn params() -> snow::params::NoiseParams {
    "Noise_XX_25519_ChaChaPoly_SHA256".parse().expect("valid noise params")
}

/// Состояние handshake (до завершения).
pub struct NoiseHandshake {
    hs: Option<HandshakeState>,
    remote_static: Option<Vec<u8>>,
}

/// Инициатор: создаёт состояние и первый message (-> e).
pub fn initiator_start(local_static_sk: &[u8; 32], prologue: &[u8]) -> Result<(NoiseHandshake, Vec<u8>), String> {
    let mut hs = Builder::new(params())
        .local_private_key(local_static_sk)
        .map_err(|e| e.to_string())?
        .prologue(prologue)
        .map_err(|e| e.to_string())?
        .build_initiator()
        .map_err(|e| e.to_string())?;
    let mut msg = vec![0u8; MAX_NOISE_MSG];
    let n = hs.write_message(&[], &mut msg).map_err(|e| e.to_string())?;
    msg.truncate(n);
    Ok((NoiseHandshake { hs: Some(hs), remote_static: None }, msg))
}

/// Инициатор с известным pubkey peer (IK-подобная оптимизация невозможна в XX — pubkey
/// передаётся в сообщениях; тем не менее держим API с remote pubkey для будущего IK).
pub fn initiator_start_with_peer(
    local_static_sk: &[u8; 32],
    peer_static_pk: &[u8; 32],
    prologue: &[u8],
) -> Result<(NoiseHandshake, Vec<u8>), String> {
    let mut hs = Builder::new(params())
        .local_private_key(local_static_sk)
        .map_err(|e| e.to_string())?
        .remote_public_key(peer_static_pk)
        .map_err(|e| e.to_string())?
        .prologue(prologue)
        .map_err(|e| e.to_string())?
        .build_initiator()
        .map_err(|e| e.to_string())?;
    let mut msg = vec![0u8; MAX_NOISE_MSG];
    let n = hs.write_message(&[], &mut msg).map_err(|e| e.to_string())?;
    msg.truncate(n);
    Ok((NoiseHandshake { hs: Some(hs), remote_static: None }, msg))
}

/// Респондер: создаёт состояние (msg1 ещё не прочитан).
pub fn responder_start(local_static_sk: &[u8; 32], prologue: &[u8]) -> Result<NoiseHandshake, String> {
    let hs = Builder::new(params())
        .local_private_key(local_static_sk)
        .map_err(|e| e.to_string())?
        .prologue(prologue)
        .map_err(|e| e.to_string())?
        .build_responder()
        .map_err(|e| e.to_string())?;
    Ok(NoiseHandshake { hs: Some(hs), remote_static: None })
}

impl NoiseHandshake {
    /// Прочитать входящее сообщение; если handshake завершён — вернуть Some(transport).
    pub fn read(&mut self, msg: &[u8]) -> Result<Option<NoiseTransport>, String> {
        let hs = self.hs.as_mut().ok_or_else(|| "handshake already consumed".to_string())?;
        let mut buf = vec![0u8; MAX_NOISE_MSG];
        let _ = hs.read_message(msg, &mut buf).map_err(|e| e.to_string())?;
        if hs.is_handshake_finished() {
            self.remote_static = hs.get_remote_static().map(|k| k.to_vec());
            let hs = self.hs.take().expect("checked above");
            let ts = hs.into_transport_mode().map_err(|e| e.to_string())?;
            Ok(Some(NoiseTransport { ts }))
        } else {
            Ok(None)
        }
    }

    /// Записать исходящее сообщение (для responder после read msg1, и т.п.).
    pub fn write(&mut self) -> Result<Option<Vec<u8>>, String> {
        let hs = match self.hs.as_mut() {
            Some(h) => h,
            None => return Ok(None),
        };
        if hs.is_handshake_finished() || !hs.is_my_turn() {
            return Ok(None);
        }
        let mut msg = vec![0u8; MAX_NOISE_MSG];
        let n = hs.write_message(&[], &mut msg).map_err(|e| e.to_string())?;
        msg.truncate(n);
        Ok(Some(msg))
    }

    /// Если handshake завершён (после последнего write/read) — забрать transport.
    pub fn take_transport(&mut self) -> Result<Option<NoiseTransport>, String> {
        let finished = self.hs.as_ref().map(|h| h.is_handshake_finished()).unwrap_or(false);
        if finished {
            if let Some(h) = self.hs.as_ref() {
                self.remote_static = h.get_remote_static().map(|k| k.to_vec());
            }
            let hs = self.hs.take().expect("checked above");
            let ts = hs.into_transport_mode().map_err(|e| e.to_string())?;
            Ok(Some(NoiseTransport { ts }))
        } else {
            Ok(None)
        }
    }

    /// Завершён ли handshake (после read вернёт transport).
    pub fn is_finished(&self) -> bool {
        self.hs.as_ref().map(|h| h.is_handshake_finished()).unwrap_or(true)
    }

    /// Peer static pubkey, если уже аутентифицирован (после 2-го сообщения в XX).
    pub fn get_remote_static(&self) -> Option<Vec<u8>> {
        self.remote_static.clone().or_else(|| {
            self.hs.as_ref().and_then(|h| h.get_remote_static().map(|k| k.to_vec()))
        })
    }
}

/// Транспорт после handshake: AEAD кадры с 2-байтовой длиной.
pub struct NoiseTransport {
    ts: TransportState,
}

impl NoiseTransport {
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        let mut out = vec![0u8; plaintext.len() + 16];
        let n = self.ts.write_message(plaintext, &mut out).map_err(|e| e.to_string())?;
        out.truncate(n);
        Ok(out)
    }

    pub fn decrypt(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        let mut out = vec![0u8; ciphertext.len()];
        let n = self.ts.read_message(ciphertext, &mut out).map_err(|e| e.to_string())?;
        out.truncate(n);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keypair() -> ([u8; 32], [u8; 32]) {
        let mut sk = [0u8; 32];
        use rand::RngCore;
        rand::rngs::OsRng.fill_bytes(&mut sk);
        let pk = x25519(sk);
        (sk, pk)
    }

    fn x25519(sk: [u8; 32]) -> [u8; 32] {
        use curve25519_dalek::constants::X25519_BASEPOINT;
        X25519_BASEPOINT.mul_clamped(sk).0
    }

    #[test]
    fn xx_handshake_and_transport_roundtrip() {
        let (a_sk, a_pk) = keypair();
        let (b_sk, b_pk) = keypair();
        let a_id = NodeId([1u8; 32]);
        let b_id = NodeId([2u8; 32]);
        let pro_a = prologue(&a_id, &b_id, 7);
        let pro_b = prologue(&b_id, &a_id, 7);
        assert_eq!(pro_a, pro_b, "prologue канонический: одинаков с обеих сторон");
        assert_ne!(pro_a, prologue(&a_id, &b_id, 8), "epoch меняет prologue");

        // initiator: msg1
        let (mut init, msg1) = initiator_start_with_peer(&a_sk, &b_pk, &pro_a).unwrap();
        // responder: read msg1 -> msg2
        let mut resp = responder_start(&b_sk, &pro_b).unwrap();
        assert!(resp.read(&msg1).unwrap().is_none());
        let msg2 = resp.write().unwrap().expect("responder turn");
        // initiator: read msg2 -> msg3
        assert!(init.read(&msg2).unwrap().is_none());
        let msg3 = init.write().unwrap().expect("initiator turn");
        // peer static виден обеим сторонам ДО потребления handshake-состояния
        assert_eq!(init.get_remote_static().unwrap(), b_pk);
        // XX: инициатор завершает handshake после ЗАПИСИ msg3
        let mut transport_a = init.take_transport().unwrap().expect("finished after write");
        let mut transport_b = resp.read(&msg3).unwrap().expect("finished");
        assert_eq!(resp.get_remote_static().unwrap(), a_pk);

        // transport roundtrip both ways
        let m = b"nexxus wire v4 payload";
        let ct = transport_a.encrypt(m).unwrap();
        assert_eq!(transport_b.decrypt(&ct).unwrap(), m);
        let ct2 = transport_b.encrypt(b"reply").unwrap();
        assert_eq!(transport_a.decrypt(&ct2).unwrap(), b"reply");
    }

    #[test]
    fn wrong_prologue_fails() {
        let (a_sk, a_pk) = keypair();
        let (b_sk, _b_pk) = keypair();
        let (mut init, msg1) = initiator_start_with_peer(&a_sk, &a_pk, b"prologue-A").unwrap();
        let mut resp = responder_start(&b_sk, b"prologue-B").unwrap();
        let _ = resp.read(&msg1).unwrap(); // msg1 = plaintext e, prologue ещё не проверен
        let msg2 = resp.write().unwrap().unwrap();
        assert!(init.read(&msg2).is_err(), "prologue mismatch должен рвать handshake на msg2");
    }

    #[test]
    fn tampered_msg3_fails() {
        // H-4: подделанный финальный handshake message рвётся AEAD-проверкой
        let (a_sk, a_pk) = keypair();
        let (b_sk, _b_pk) = keypair();
        let (mut init, msg1) = initiator_start_with_peer(&a_sk, &a_pk, b"p").unwrap();
        let mut resp = responder_start(&b_sk, b"p").unwrap();
        let _ = resp.read(&msg1).unwrap();
        let msg2 = resp.write().unwrap().unwrap();
        let _ = init.read(&msg2).unwrap();
        let mut msg3 = init.write().unwrap().unwrap();
        msg3[0] ^= 0xff; // tamper
        assert!(resp.read(&msg3).is_err(), "tampered handshake message должен рваться");
    }
}
