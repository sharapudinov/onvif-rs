use std::fmt;

/// A SOAP 1.2 Fault returned by a device, with everything a caller may need to branch on.
///
/// Code and subcodes are kept as the raw QName text sent by the device (e.g. `s:Sender`,
/// `ter:NotAuthorized`): the prefix is chosen by the device and is meaningless on its own,
/// so match on the local part through [`SoapFault::has_subcode`] / [`local_name`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoapFault {
    /// `Code/Value`, e.g. `s:Sender`.
    pub code: String,
    /// `Code/Subcode/Value` chain flattened outermost first, e.g. `["ter:InvalidArgVal"]`.
    pub subcodes: Vec<String>,
    /// Text of every `Reason/Text`, in document order.
    pub reasons: Vec<String>,
    /// Direct element children of `Detail`. Some devices (Dahua) report the fault type here
    /// instead of in a subcode, e.g. `wsnt:SubscribeCreationFailedFault`.
    pub detail_elements: Vec<FaultDetailElement>,
    /// HTTP status of the response that carried the fault.
    pub http_status: Option<u16>,
}

/// A namespace-resolved element name found inside a fault's `Detail`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaultDetailElement {
    pub namespace: Option<String>,
    pub name: String,
}

/// The part of a QName after the last `:`.
pub fn local_name(qname: &str) -> &str {
    qname.rsplit(':').next().unwrap_or(qname)
}

impl SoapFault {
    /// ONVIF reports authorization failures as a `NotAuthorized` subcode.
    pub fn is_unauthorized(&self) -> bool {
        self.subcodes.iter().any(|s| s.contains("NotAuthorized"))
    }

    /// True when any subcode has the given local name, whatever its prefix.
    pub fn has_subcode(&self, local: &str) -> bool {
        self.subcodes.iter().any(|s| local_name(s) == local)
    }

    /// True when `Detail` has a direct child with the given local name, whatever its namespace.
    pub fn has_detail_element(&self, local: &str) -> bool {
        self.detail_elements.iter().any(|e| e.name == local)
    }

    /// True when the fault is identified by `local` either as a subcode or as a detail element.
    pub fn is_fault_type(&self, local: &str) -> bool {
        self.has_subcode(local) || self.has_detail_element(local)
    }
}

impl fmt::Display for SoapFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.code)?;
        for subcode in &self.subcodes {
            write!(f, "/{subcode}")?;
        }
        if let Some(status) = self.http_status {
            write!(f, " (HTTP {status})")?;
        }
        if !self.reasons.is_empty() {
            write!(f, ": {}", self.reasons.join("; "))?;
        }
        if !self.detail_elements.is_empty() {
            let names: Vec<&str> = self
                .detail_elements
                .iter()
                .map(|e| e.name.as_str())
                .collect();
            write!(f, " [detail: {}]", names.join(", "))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dahua_fault() -> SoapFault {
        SoapFault {
            code: "s:Sender".into(),
            subcodes: vec![],
            reasons: vec!["Subscribe Creation Failed".into()],
            detail_elements: vec![FaultDetailElement {
                namespace: Some("http://docs.oasis-open.org/wsn/b-2".into()),
                name: "SubscribeCreationFailedFault".into(),
            }],
            http_status: Some(500),
        }
    }

    #[test]
    fn display_renders_code_subcodes_reason_and_detail() {
        let mut fault = dahua_fault();
        fault.subcodes = vec!["ter:InvalidArgVal".into(), "ter:Other".into()];
        assert_eq!(
            fault.to_string(),
            "s:Sender/ter:InvalidArgVal/ter:Other (HTTP 500): Subscribe Creation Failed \
             [detail: SubscribeCreationFailedFault]"
        );
    }

    #[test]
    fn a_fault_type_is_found_by_local_name_in_subcodes_and_in_detail() {
        let mut fault = dahua_fault();
        assert!(fault.is_fault_type("SubscribeCreationFailedFault"));
        assert!(!fault.has_subcode("SubscribeCreationFailedFault"));
        assert!(!fault.is_fault_type("Other"));

        fault.detail_elements.clear();
        fault.subcodes = vec!["wsnt:SubscribeCreationFailedFault".into()];
        assert!(fault.is_fault_type("SubscribeCreationFailedFault"));
        assert!(!fault.has_detail_element("SubscribeCreationFailedFault"));
    }
}
