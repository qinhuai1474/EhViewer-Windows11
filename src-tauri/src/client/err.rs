//! Exception classification ported from SXJ's exception model.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum EhError {
    /// The site returned HTTP 509 (bandwidth exceeded for anonymous users).
    #[error("509 Bandwidth Exceeded - 请稍后再试")]
    BandwidthExceeded,
    /// The gallery has been removed / is offensive.
    #[error("该画廊不存在或已被移除")]
    GalleryNotFound,
    /// The server requires a (proper) cookie / captcha.
    #[error("需要有效 Cookie 或遇到验证码：{0}")]
    Validation(String),
    /// The list is empty (no hits found).
    #[error("没有结果")]
    EmptyList,
    /// HTML/JSON could not be parsed into the expected structure.
    #[error("解析失败：{0}")]
    Parse(String),
    /// A network-level failure (timeout, connection, TLS).
    #[error("网络错误：{0}")]
    Network(String),
    /// Any other internal error.
    #[error("内部错误：{0}")]
    Internal(String),
}

impl EhError {
    /// Classifies a raw HTML body into a typed error, mirroring SXJ behaviour.
    pub fn classify(body: &str) -> Option<Self> {
        let lower = body.to_lowercase();
        // Cloudflare challenge / 400 block (case-insensitive; CF bodies mix casing).
        if lower.contains("cloudflare") || lower.contains("just a moment...") {
            Some(Self::Validation("Cloudflare 验证拦截，请检查网络、代理或稍后重试".into()))
        } else if body.contains("509 Bandwidth Limit Exceeded")
            || body.contains("Bandwidth Exceeded")
        {
            Some(Self::BandwidthExceeded)
        } else if body.contains("Offensive Content") || body.contains("has been removed") {
            Some(Self::GalleryNotFound)
        } else {
            None
        }
    }

    /// Whether the error is transient and worth an automatic retry (bandwidth
    /// exceeded and plain network failures; everything else must surface).
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Network(_) | Self::BandwidthExceeded)
    }
}

pub type EhResult<T> = Result<T, EhError>;


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_cloudflare_400_lowercase() {
        let body = "<html><head><title>400 Bad Request</title></head><body><h1>400 Bad Request</h1><hr><center>cloudflare</center></body></html>";
        assert!(matches!(EhError::classify(body), Some(EhError::Validation(_))));
    }

    #[test]
    fn classify_cloudflare_challenge() {
        let body = "<html><body>Just a moment...</body></html>";
        assert!(matches!(EhError::classify(body), Some(EhError::Validation(_))));
    }

    #[test]
    fn classify_normal_body_none() {
        assert!(EhError::classify("<html>normal page</html>").is_none());
    }
}
