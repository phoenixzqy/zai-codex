use std::io;
use std::io::Write;

use flate2::Compression;
use flate2::write::GzEncoder;
use sentry::protocol::Envelope;

pub(super) fn encode_envelope(envelope: &Envelope) -> io::Result<(Vec<u8>, usize)> {
    let mut writer = CountingWriter {
        inner: GzEncoder::new(Vec::new(), Compression::fast()),
        bytes: 0,
    };
    envelope.to_writer(&mut writer)?;
    let decoded_bytes = writer.bytes;
    let mut body = writer.inner.finish()?;
    // Already-compressed attachments can grow beyond Sentry's 200 MiB wire limit.
    // Reuse the buffer for the raw envelope when gzip does not make it smaller.
    if body.len() >= decoded_bytes {
        body.clear();
        envelope.to_writer(&mut body)?;
    }
    Ok((body, decoded_bytes))
}

struct CountingWriter<W> {
    inner: W,
    bytes: usize,
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(buf)?;
        self.bytes += written;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
