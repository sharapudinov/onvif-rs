use super::Error;
use reqwest::StatusCode;
use schema::transport::{Error as TransportError, FaultDetailElement, SoapFault};
use xmltree::{Element, XMLNode};

fn child_elements<'a>(parent: &'a Element, name: &'a str) -> impl Iterator<Item = &'a Element> {
    parent.children.iter().filter_map(move |node| match node {
        XMLNode::Element(e) if e.name == name => Some(e),
        _ => None,
    })
}

fn text_of(element: &Element) -> String {
    element
        .get_text()
        .map(|t| t.trim().to_string())
        .unwrap_or_default()
}

/// Reads a `s:Fault` element by hand: the generated `soap_envelope::Fault` drops reason text,
/// nested subcodes and every `Detail` child, which are exactly what callers branch on.
pub(super) fn parse_fault(fault: &Element) -> Result<SoapFault, Error> {
    let code_element = fault
        .get_child("Code")
        .ok_or_else(|| Error::InternalError("SOAP Fault without Code".to_string()))?;
    let code = code_element
        .get_child("Value")
        .map(text_of)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| Error::InternalError("SOAP Fault Code without Value".to_string()))?;

    let mut subcodes = Vec::new();
    let mut current = code_element.get_child("Subcode");
    while let Some(subcode) = current {
        if let Some(value) = subcode.get_child("Value").map(text_of) {
            subcodes.push(value);
        }
        current = subcode.get_child("Subcode");
    }

    let reasons = fault
        .get_child("Reason")
        .map(|reason| child_elements(reason, "Text").map(text_of).collect())
        .unwrap_or_default();

    let detail_elements = fault
        .get_child("Detail")
        .map(|detail| {
            detail
                .children
                .iter()
                .filter_map(|node| match node {
                    XMLNode::Element(e) => Some(FaultDetailElement {
                        namespace: e.namespace.clone(),
                        name: e.name.clone(),
                    }),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(SoapFault {
        code,
        subcodes,
        reasons,
        detail_elements,
        http_status: None,
    })
}

/// Transport error for a fault received with the given HTTP status.
pub(super) fn fault_error(mut fault: SoapFault, status: StatusCode) -> TransportError {
    fault.http_status = Some(status.as_u16());
    TransportError::Fault(fault)
}

/// Error for a non-2xx, non-401, non-redirect response. A NotAuthorized fault stays an
/// authorization error (it triggers the UsernameToken fallback); any other fault is returned
/// structurally; a body that is not a SOAP Fault keeps the bare status.
pub(super) fn error_for_failed_status(status: StatusCode, body: &str) -> TransportError {
    match super::unsoap(body) {
        Err(Error::Fault(fault)) if fault.is_unauthorized() => {
            TransportError::Authorization("Unauthorized".to_string())
        }
        Err(Error::Fault(fault)) => fault_error(*fault, status),
        _ => TransportError::Other(status.to_string()),
    }
}
