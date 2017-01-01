use std::io::{self, Read};
use std::ops::Deref;

use decoder::*;

use reqwest;
use reqwest::header::{ContentType, Headers};
use xml::escape::escape_str_attribute;
use xml::reader::{Error as ReaderError, EventReader, XmlEvent as ReaderEvent};
use xml::writer::{EmitterConfig, Result as XmlResult, XmlEvent};

const SVUE_ENDPOINT: &'static str = "https://student-portland.cascadetech.org/portland/Service/PXPCommunication.asmx";
const SOAP_ACTION: &'static [u8; 56] = b"http://edupoint.com/webservices/ProcessWebServiceRequest";

#[derive(Clone)]
pub enum SVUEAPIAction {
    RetrieveGrades(Option<i8>),
    RetrieveStudentInfo,
}

impl SVUEAPIAction {
    fn as_str(&self) -> &'static str {
        match *self {
            SVUEAPIAction::RetrieveGrades(_) => "Gradebook",
            SVUEAPIAction::RetrieveStudentInfo => "ChildList",
        }
    }
}

pub struct SVUERequest<'a> {
    action: SVUEAPIAction,
    credentials: (&'a str, &'a str),
}

#[derive(Debug)]
pub struct DecodedSVUEError {
    error_message: String,
    stack_trace: String,
}

impl DecodedSVUEError {
    fn decode(xml: String) -> DecoderResult<DecodedSVUEError> {
        let mut error = None;
        let mut stack_trace = None;

        {
            let reader = EventReader::new(xml.as_bytes());

            for e in reader {
                match e {
                    Ok(e) => {
                        match e {
                            ReaderEvent::StartElement { name, attributes, .. } => {
                                match name.local_name.as_str() {
                                    "RT_ERROR" => {
                                        let attrs = attributes_vec_to_map(&attributes);
                                        error = Some(get_attr_owned!(attrs, "ERROR_MESSAGE"));
                                    }
                                    _ => {}
                                }
                            }
                            ReaderEvent::EndElement { name, .. } => {
                                match name.local_name.as_str() {
                                    "RT_ERROR" => { break; }
                                    _ => {}
                                }
                            }
                            ReaderEvent::Characters(cs) => { stack_trace = Some(cs); }
                            ReaderEvent::StartDocument { .. } => {}
                            ReaderEvent::Whitespace(_) => {}
                            _ => { return Err(DecodingError::UnexpectedEvent(e)); }
                        }
                    }
                    Err(e) => { return Err(DecodingError::EventError(e)); }
                }
            }
        }

        match (error, stack_trace) {
            (Some(e), Some(st)) => {
                Ok(DecodedSVUEError {
                    error_message: e,
                    stack_trace: st,
                })
            }
            (None, _) | (_, None) => Err(DecodingError::SVUEErrorParsingFailed(xml))
        }
    }
}

#[derive(Debug)]
pub enum SVUERequestError {
    DecodingError(DecodingError),
    ExpectedTagNotFound(String),
    RawDecodingError(ReaderError),
    ReqwestError(reqwest::Error),
    ResponseBodyNotFound,
    ResponseReadError(io::Error),
    SVUEError(DecodedSVUEError),
    SVUEErrorParsingFailed(DecodingError),
}

pub struct SVUEResponse {
    pub req_action: SVUEAPIAction,
    pub xml: String,
}

impl SVUEResponse {
    fn new_from_raw<'a>(raw: &'a str, expect: &'a str, action: SVUEAPIAction) -> Result<SVUEResponse, SVUERequestError> {
        let xml = Self::decode_raw(raw, expect)?;

        Ok(SVUEResponse {
            req_action: action,
            xml: xml,
        })
    }

    fn decode_raw<'a>(raw: &'a str, expect: &'a str) -> Result<String, SVUERequestError> {
        let reader = EventReader::new(raw.as_bytes());

        for e in reader {
            match e {
                Ok(ReaderEvent::Characters(cs)) => { return Self::get_expected_xml(cs, expect); }
                Ok(_) => {}
                Err(e) => { return Err(SVUERequestError::RawDecodingError(e)); }
            }
        }

        Err(SVUERequestError::ResponseBodyNotFound)
    }

    fn get_expected_xml<'a>(xml: String, expect: &'a str) -> Result<String, SVUERequestError> {
        let mut found = false;
        let mut error = false;

        {
            let reader = EventReader::new(xml.as_bytes());

            for e in reader {
                match e {
                    Ok(ReaderEvent::StartElement { ref name, .. }) => {
                        match name.local_name.as_str() {
                            x if expect == x => { found = true; break; }
                            "RT_ERROR" => { error = true; break; }
                            _ => {}
                        }
                    }
                    Ok(_) => {}
                    Err(e) => { return Err(SVUERequestError::RawDecodingError(e)); }
                }
            }
        }

        if found {
            Ok(xml)
        } else {
            if error {
                let err = DecodedSVUEError::decode(xml)
                    .map_err(|e| SVUERequestError::SVUEErrorParsingFailed(e))?;

                Err(SVUERequestError::SVUEError(err))
            } else {
                Err(SVUERequestError::ExpectedTagNotFound(expect.to_string()))
            }
        }
    }
}

macro_rules! write_element {
    ( $w:expr; $element:expr => $inner_text:expr ) => {
        $w.write(XmlEvent::start_element($element))?;
        $w.write(XmlEvent::characters($inner_text))?;
        $w.write(XmlEvent::end_element())?;
    };
}

impl<'a> SVUERequest<'a> {
    pub fn perform(action: SVUEAPIAction, creds: (&'a str, &'a str)) -> Result<SVUEResponse, SVUERequestError> {
        let req = SVUERequest {
            action: action,
            credentials: creds,
        };

        req.run()
    }

    fn run(&self) -> Result<SVUEResponse, SVUERequestError> {
        let body = self.build_body().unwrap();
        let client = reqwest::Client::new().unwrap();

        let mut headers = Headers::new();
        headers.set(ContentType("text/xml; charset=utf-8".parse().unwrap()));
        headers.set_raw("SOAPAction", vec![SOAP_ACTION.to_vec()]);

        let mut buffer = String::new();
        client.post(SVUE_ENDPOINT)
            .headers(headers)
            .body(body)
            .send()
            .map_err(|e| SVUERequestError::ReqwestError(e))
            .map(|mut r| {
                r.read_to_string(&mut buffer)
                    .map(|_| SVUEResponse::new_from_raw(&buffer, self.action.as_str(), self.action.clone()))
                    .map_err(|e| SVUERequestError::ResponseReadError(e))?
            })?
    }

    fn build_body(&self) -> XmlResult<Vec<u8>> {
        let mut buffer = Vec::new();

        {
            let mut c = EmitterConfig::new()
                .perform_indent(true)
                .indent_string("    ")
                .normalize_empty_elements(false);
            // For characters, xml-rs only does automatic escaping for PCDATA sections, which
            // includes just `"` and `<`. This is a problem since the StudentVUE API requires
            // `paramStr` to be attribute-escaped.
            // https://github.com/netvl/xml-rs/blob/master/src/escape.rs#L110
            c.perform_escaping = false;
            let mut w = c.create_writer(&mut buffer);

            let root = XmlEvent::start_element("soap:Envelope")
                .ns("xsi", "http://www.w3.org/2001/XMLSchema-instance")
                .ns("xsd", "http://www.w3.org/2001/XMLSchema")
                .ns("soap", "http://schemas.xmlsoap.org/soap/envelope/");
            w.write(root)?;
            let body = XmlEvent::start_element("soap:Body");
            w.write(body)?;
            let req = XmlEvent::start_element("ProcessWebServiceRequest")
                .ns("", "http://edupoint.com/webservices/");
            w.write(req)?;
            write_element! { w; "userID" => &self.credentials.0 };
            write_element! { w; "password" => &self.credentials.1 };
            write_element! { w; "skipLoginLog" => "1" };
            write_element! { w; "parent" => "0" };
            write_element! { w; "webServiceHandleName" => "PXPWebServices" };
            write_element! { w; "methodName" => self.action.as_str() };

            let params = self.build_params().unwrap();
            write_element! { w; "paramStr" => escape_str_attribute(&params).deref() };
            w.write(XmlEvent::end_element())?;
            w.write(XmlEvent::end_element())?;
            w.write(XmlEvent::end_element())?;
        }

        Ok(buffer)
    }

    fn build_params(&self) -> XmlResult<String> {
        let mut buffer = Vec::new();

        {
            let mut c = EmitterConfig::new()
                .write_document_declaration(false)
                .normalize_empty_elements(false);
            c.perform_escaping = false;
            let mut w = c.create_writer(&mut buffer);

            let params = XmlEvent::start_element("Parms");
            w.write(params)?;
            write_element! { w; "ChildIntID" => "0" };

            match self.action {
                SVUEAPIAction::RetrieveGrades(idx) => {
                    if idx.is_some() {
                        let idx = idx.unwrap().to_string();
                        write_element! { w; "ReportPeriod" => &idx };
                    }
                }
                _ => {}
            }
            w.write(XmlEvent::end_element())?;
        }

        Ok(String::from_utf8(buffer).unwrap())
    }
}
