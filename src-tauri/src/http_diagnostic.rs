pub(crate) fn transport_category(error: &minreq::Error) -> &'static str {
    match error {
        minreq::Error::AddressNotFound => "dns",
        minreq::Error::BadProxy
        | minreq::Error::BadProxyCreds
        | minreq::Error::ProxyConnect
        | minreq::Error::InvalidProxyCreds => "proxy",
        minreq::Error::RustlsCreateConnection(_) => "tls",
        minreq::Error::IoError(_) => "network/tls",
        _ => "network",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_categories_do_not_echo_request_context() {
        assert_eq!(transport_category(&minreq::Error::AddressNotFound), "dns");
        assert_eq!(transport_category(&minreq::Error::ProxyConnect), "proxy");
    }
}
