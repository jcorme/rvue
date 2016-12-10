use std::str::FromStr;

use api::{SVUEAPIAction, SVUERequest, SVUERequestError};
use decoder::*;
use diff::Pairable;

use chrono::NaiveDate;
use regex::Regex;
use xml::reader::{Events, EventReader, XmlEvent as ReaderEvent};

macro_rules! field_slice_helpers {
    ( $t:ty, { $($field:tt => $field_t:ty),+ } ) => {
        impl $t {
            $(
                #[allow(dead_code)]
                pub fn $field(&self) -> &[$field_t] {
                    self.$field.as_slice()
                }
            )+
        }
    };
}

#[derive(Clone, Debug)]
pub struct Gradebook {
    pub courses: Vec<Course>,
    pub reporting_period: ReportingPeriod,
    pub reporting_periods: Vec<ReportPeriod>,
}

field_slice_helpers!(Gradebook, {
    courses => Course,
    reporting_periods => ReportPeriod
});

impl Gradebook {
    pub fn retrieve<'a>(user: &'a str, password: &'a str) -> Result<Gradebook, SVUERequestError> {
        Self::retrieve_for_grade_period(user, password, -1)
    }

    pub fn retrieve_for_grade_period<'a>(user: &'a str, password: &'a str, period: i8) -> Result<Gradebook, SVUERequestError> {
        let action = if period < 0 {
            SVUEAPIAction::RetrieveGrades(None)
        } else {
            SVUEAPIAction::RetrieveGrades(Some(period))
        };
        let resp = SVUERequest::perform(action, (user, password))?;
        let mut events_iter = EventReader::new(resp.xml.as_bytes()).into_iter();

        Self::from_event(events_iter.next().unwrap().unwrap(), &mut events_iter)
            .map_err(|e| SVUERequestError::DecodingError(e))
    }
}

impl SVUEDecodeable for Gradebook {
    fn from_event(_: ReaderEvent, events_iter: &mut Events<&[u8]>) -> DecoderResult<Gradebook> {
        let mut courses = Vec::new();
        let mut reporting_period: ReportingPeriod = Default::default();
        let mut reporting_periods = Vec::new();

        loop {
            match events_iter.next() {
                Some(Ok(event)) => {
                    match event.clone() {
                        ReaderEvent::StartElement { ref name, .. } => {
                            match name.local_name.as_str() {
                                "Course" => {
                                    let course = Course::from_event(event, events_iter)?;

                                    courses.push(course);
                                }
                                "ReportPeriod" => {
                                    let report_period = ReportPeriod::from_event(event, events_iter)?;

                                    reporting_periods.push(report_period);
                                }
                                "ReportingPeriod" => {
                                    reporting_period = ReportingPeriod::from_event(event, events_iter)?;
                                }
                                _ => {}
                            }
                        }
                        ReaderEvent::EndElement { name } => {
                            match name.local_name.as_str() {
                                "Gradebook" => {
                                    return Ok(Gradebook {
                                        courses: courses,
                                        reporting_period: reporting_period,
                                        reporting_periods: reporting_periods,
                                    });
                                }
                                _ => {}
                            }
                        }
                        ReaderEvent::Whitespace(_) => {},
                        _ => {}
                    }
                }
                Some(Err(e)) => { return Err(DecodingError::EventError(e)); }
                None => { return Err(DecodingError::UnexpectedEnd); }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct ReportPeriod {
    pub end_date: NaiveDate,
    pub grade_period: String,
    pub index: i8,
    pub start_date: NaiveDate,
}

impl SVUEDecodeable for ReportPeriod {
    fn from_event(event: ReaderEvent, _: &mut Events<&[u8]>) -> DecoderResult<ReportPeriod> {
        match event.clone() {
            ReaderEvent::StartElement { name, attributes, .. } => {
                match name.local_name.as_str() {
                    "ReportPeriod" => {
                        let attrs = attributes_vec_to_map(&attributes);

                        Ok(ReportPeriod {
                            end_date: parse_date!(attrs, "EndDate"),
                            grade_period: get_attr_owned!(attrs, "GradePeriod").clone(),
                            index: parse_int!(i8, attrs, "Index"),
                            start_date: parse_date!(attrs, "StartDate"),
                        })
                    }
                    _ => Err(DecodingError::UnexpectedEvent(event))
                }
            }
            _ => Err(DecodingError::UnexpectedEvent(event))
        }
    }
}

#[derive(Clone, Debug)]
pub struct ReportingPeriod {
    pub end_date: NaiveDate,
    pub grade_period: String,
    pub start_date: NaiveDate,
}

impl Default for ReportingPeriod {
    fn default() -> ReportingPeriod {
        ReportingPeriod {
            end_date: NaiveDate::from_ymd(1970, 1, 1),
            grade_period: "".to_string(),
            start_date: NaiveDate::from_ymd(1970, 1, 1),
        }
    }
}

impl SVUEDecodeable for ReportingPeriod {
    fn from_event(event: ReaderEvent, _: &mut Events<&[u8]>) -> DecoderResult<ReportingPeriod> {
        match event.clone() {
            ReaderEvent::StartElement { name, attributes, .. } => {
                match name.local_name.as_str() {
                    "ReportingPeriod" => {
                        let attrs = attributes_vec_to_map(&attributes);

                        Ok(ReportingPeriod {
                            end_date: parse_date!(attrs, "EndDate"),
                            grade_period: get_attr_owned!(attrs, "GradePeriod"),
                            start_date: parse_date!(attrs, "StartDate"),
                        })
                    }
                    _ => Err(DecodingError::UnexpectedEvent(event))
                }
            }
            _ => Err(DecodingError::UnexpectedEvent(event))
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum CourseTitle {
    Parsed(String, String),
    Unparseable(String),
}

impl CourseTitle {
    fn parse(title: &str) -> CourseTitle {
        let r = Regex::new(r"(.+)\s+\((.+?)\)").unwrap();
        let captures = r.captures(title);

        match captures {
            Some(cs) => {
                let name = match cs.at(1) {
                    Some(n) => n.to_string(),
                    None => return CourseTitle::Unparseable(title.to_string()),
                };
                let id = match cs.at(2) {
                    Some(id) => id.to_string(),
                    None => return CourseTitle::Unparseable(title.to_string()),
                };

                CourseTitle::Parsed(name, id)
            }
            None => CourseTitle::Unparseable(title.to_string())
        }
    }
}

#[derive(Clone, Debug)]
pub struct Course {
    pub highlight_percentage_cut_off_for_progress_bar: i8,
    pub marks: Vec<Mark>,
    pub period: i8,
    pub room: String,
    pub staff: String,
    pub staff_email: String,
    pub title: CourseTitle,
}

field_slice_helpers!(Course, {
    marks => Mark
});

impl<'a> Pairable<'a, CourseTitle> for Course {
    fn unique_key(&'a self) -> &'a CourseTitle {
        &self.title
    }
}

impl SVUEDecodeable for Course {
    fn from_event(event: ReaderEvent, events_iter: &mut Events<&[u8]>) -> DecoderResult<Course> {
        match event.clone() {
            ReaderEvent::StartElement { name, attributes, .. } => {
                match name.local_name.as_str() {
                    "Course" => {
                        let attrs = attributes_vec_to_map(&attributes);

                        let cutoff = parse_int!(i8, attrs, "HighlightPercentageCutOffForProgressBar");
                        let mut marks = Vec::new();

                        loop {
                            match events_iter.next() {
                                Some(Ok(event)) => {
                                    match event.clone() {
                                        ReaderEvent::StartElement { name, .. } => {
                                            match name.local_name.as_str() {
                                                "Mark" => {
                                                    let mark = Mark::from_event(event, events_iter)?;

                                                    marks.push(mark);
                                                }
                                                "Marks" => {},
                                                _ => { return Err(DecodingError::UnexpectedEvent(event)); }
                                            }
                                        }
                                        ReaderEvent::EndElement { name, .. } => {
                                            match name.local_name.as_str() {
                                                "Course" => {
                                                    break;
                                                }
                                                _ => {}
                                            }
                                        }
                                        ReaderEvent::Whitespace(_) => {},
                                        _ => { return Err(DecodingError::UnexpectedEvent(event)); }
                                    }
                                }
                                Some(Err(e)) => { return Err(DecodingError::EventError(e)); },
                                None => { return Err(DecodingError::UnexpectedEnd); }
                            }
                        }

                        let period = parse_int!(i8, attrs, "Period");
                        let room = get_attr_owned!(attrs, "Room");
                        let staff = get_attr_owned!(attrs, "Staff");
                        let staff_email = get_attr_owned!(attrs, "StaffEMail");
                        let title = CourseTitle::parse(get_attr!(attrs, "Title"));

                        Ok(Course {
                            highlight_percentage_cut_off_for_progress_bar: cutoff,
                            marks: marks,
                            period: period,
                            room: room,
                            staff: staff,
                            staff_email: staff_email,
                            title: title,
                        })
                    }
                    _ => Err(DecodingError::UnexpectedEvent(event))
                }
            }
            _ => Err(DecodingError::UnexpectedEvent(event))
        }
    }
}

#[derive(Clone, Debug)]
pub struct Mark {
    pub assignments: Vec<Assignment>,
    pub calculated_score_raw: f64,
    pub calculated_score_string: String,
    pub grade_calculation_summary: Vec<AssignmentGradeCalc>,
    pub mark_name: String,
    pub standard_views: Vec<StandardView>,
}

field_slice_helpers!(Mark, {
    assignments => Assignment,
    grade_calculation_summary => AssignmentGradeCalc,
    standard_views => StandardView
});

impl SVUEDecodeable for Mark {
    fn from_event(event: ReaderEvent, events_iter: &mut Events<&[u8]>) -> DecoderResult<Mark> {
        match event.clone() {
            ReaderEvent::StartElement { name, attributes, .. } => {
                match name.local_name.as_str() {
                    "Mark" => {
                        let attrs = attributes_vec_to_map(&attributes);

                        let mut assignments = Vec::new();
                        let mut grade_calculation_summary = Vec::new();
                        let mut standard_views = Vec::new();

                        loop {
                            match events_iter.next() {
                                Some(Ok(event)) => {
                                    match event.clone() {
                                        ReaderEvent::StartElement { name, .. } => {
                                            match name.local_name.as_str() {
                                                "Assignment" => {
                                                    let a = Assignment::from_event(event, events_iter)?;
                                                    assignments.push(a);
                                                }
                                                "Assignments" => {}
                                                "AssignmentGradeCalc" => {
                                                    let agc = AssignmentGradeCalc::from_event(event, events_iter)?;
                                                    grade_calculation_summary.push(agc);
                                                }
                                                "GradeCalculationSummary" => {}
                                                "StandardView" => {
                                                    let sv = StandardView::from_event(event, events_iter)?;
                                                    standard_views.push(sv);
                                                }
                                                "StandardViews" => {}
                                                _ => { return Err(DecodingError::UnexpectedEvent(event)); }
                                            }
                                        }
                                        ReaderEvent::EndElement { name, .. } => {
                                            match name.local_name.as_str() {
                                                "Mark" => {
                                                    break;
                                                }
                                                _ => {}
                                            }
                                        }
                                        ReaderEvent::Whitespace(_) => {},
                                        _ => { return Err(DecodingError::UnexpectedEvent(event)); }
                                    }
                                }
                                Some(Err(e)) => { return Err(DecodingError::EventError(e)); }
                                None => { return Err(DecodingError::UnexpectedEnd); }
                            }
                        }

                        let calculated_score_raw = parse_float!(f64, attrs, "CalculatedScoreRaw");
                        let calculated_score_string = get_attr_owned!(attrs, "CalculatedScoreString");
                        let mark_name = get_attr_owned!(attrs, "MarkName");

                        Ok(Mark {
                            assignments: assignments,
                            mark_name: mark_name,
                            calculated_score_raw: calculated_score_raw,
                            calculated_score_string: calculated_score_string,
                            grade_calculation_summary: grade_calculation_summary,
                            standard_views: standard_views,
                        })
                    }
                    _ => Err(DecodingError::UnexpectedEvent(event))
                }
            }
            _ => Err(DecodingError::UnexpectedEvent(event))
        }
    }
}

#[derive(Clone, Debug)]
pub struct StandardView {
    pub cal_value: f64,
    pub description: String,
    pub mark: String,
    pub proficiency: Option<f64>,
    pub proficiency_max_value: f64,
    pub standard_assignment_views: Vec<StandardAssignmentView>,
    pub subject: String,
    pub subject_id: i8,
}

field_slice_helpers!(StandardView, {
    standard_assignment_views => StandardAssignmentView
});

impl SVUEDecodeable for StandardView {
    fn from_event(event: ReaderEvent, events_iter: &mut Events<&[u8]>) -> DecoderResult<StandardView> {
        match event.clone() {
            ReaderEvent::StartElement { name, attributes, .. } => {
                match name.local_name.as_str() {
                    "StandardView" => {
                        let attrs = attributes_vec_to_map(&attributes);

                        let cal_value = parse_float!(f64, attrs, "CalValue");
                        let description = get_attr_owned!(attrs, "Description");
                        let mark = get_attr_owned!(attrs, "Mark");
                        let proficiency = f64::from_str(get_attr!(attrs, "Proficiency")).ok();
                        let proficiency_max_value = parse_float!(f64, attrs, "ProfciencyMaxValue");
                        let mut standard_assignment_views = Vec::new();

                        loop {
                            match events_iter.next() {
                                Some(Ok(event)) => {
                                    match event.clone() {
                                        ReaderEvent::StartElement { name, .. } => {
                                            match name.local_name.as_str() {
                                                "StandardAssignmentView" => {
                                                    let sav = StandardAssignmentView::from_event(event, events_iter)?;
                                                    standard_assignment_views.push(sav);
                                                }
                                                "StandardAssignmentViews" => {},
                                                _ => { return Err(DecodingError::UnexpectedEvent(event)); }
                                            }
                                        }
                                        ReaderEvent::EndElement { name, .. } => {
                                            match name.local_name.as_str() {
                                                "StandardAssignmentViews" => {
                                                    break;
                                                }
                                                _ => {}
                                            }
                                        }
                                        ReaderEvent::Whitespace(_) => {},
                                        _ => { return Err(DecodingError::UnexpectedEvent(event)); }
                                    }
                                }
                                Some(Err(e)) => { return Err(DecodingError::EventError(e)); }
                                None => { return Err(DecodingError::UnexpectedEnd); }
                            }
                        }

                        let subject = get_attr_owned!(attrs, "Subject");
                        let subject_id = parse_int!(i8, attrs, "SubjectID");

                        Ok(StandardView {
                            cal_value: cal_value,
                            description: description,
                            mark: mark,
                            proficiency: proficiency,
                            proficiency_max_value: proficiency_max_value,
                            standard_assignment_views: standard_assignment_views,
                            subject: subject,
                            subject_id: subject_id,
                        })
                    }
                    _ => Err(DecodingError::UnexpectedEvent(event))
                }
            }
            _ => Err(DecodingError::UnexpectedEvent(event))
        }
    }
}

#[derive(Clone, Debug)]
pub struct StandardAssignmentView {
    pub _type: String,
    pub assignment: String,
    pub cal_value: f64,
    pub due_date: NaiveDate,
    pub gradebook_id: String,
    pub mark: String,
    pub proficiency: Option<f64>,
    pub proficiency_max_value: f64,
}

impl SVUEDecodeable for StandardAssignmentView {
    fn from_event(event: ReaderEvent, _: &mut Events<&[u8]>) -> DecoderResult<StandardAssignmentView> {
        match event.clone() {
            ReaderEvent::StartElement { name, attributes, .. } => {
                match name.local_name.as_str() {
                    "StandardAssignmentView" => {
                        let attrs = attributes_vec_to_map(&attributes);

                        let _type = get_attr_owned!(attrs, "Type");
                        let assignment = get_attr_owned!(attrs, "Assignment");
                        let cal_value = parse_float!(f64, attrs, "CalValue");
                        let due_date = parse_date!(attrs, "DueDate");
                        let gradebook_id = get_attr_owned!(attrs, "GradebookID");
                        let mark = get_attr_owned!(attrs, "Mark");
                        let proficiency = f64::from_str(get_attr!(attrs, "Proficiency")).ok();
                        // they can't even fucking spell Proficiency correctly
                        let proficiency_max_value = parse_float!(f64, attrs, "ProfciencyMaxValue");

                        Ok(StandardAssignmentView {
                            _type: _type,
                            assignment: assignment,
                            cal_value: cal_value,
                            due_date: due_date,
                            gradebook_id: gradebook_id,
                            mark: mark,
                            proficiency: proficiency,
                            proficiency_max_value: proficiency_max_value,
                        })
                    }
                    _ => Err(DecodingError::UnexpectedEvent(event))
                }
            }
            _ => Err(DecodingError::UnexpectedEvent(event))
        }
    }
}

#[derive(Clone, Debug)]
pub struct AssignmentGradeCalc {
    pub _type: String,
    pub calculated_mark: String,
    pub points: f64,
    pub points_possible: f64,
    // this is a percentage
    pub weight: AssignmentGradeCalcWeight,
    pub weighted_pct: AssignmentGradeCalcWeight,
}

impl SVUEDecodeable for AssignmentGradeCalc {
    fn from_event(event: ReaderEvent, _: &mut Events<&[u8]>) -> DecoderResult<AssignmentGradeCalc> {
        match event.clone() {
            ReaderEvent::StartElement { name, attributes, .. } => {
                match name.local_name.as_str() {
                    "AssignmentGradeCalc" => {
                        let attrs = attributes_vec_to_map(&attributes);

                        let _type = get_attr_owned!(attrs, "Type");
                        let calculated_mark = get_attr_owned!(attrs, "CalculatedMark");
                        let points = parse_float!(f64, attrs, "Points");
                        let points_possible = parse_float!(f64, attrs, "PointsPossible");
                        let weight = AssignmentGradeCalcWeight::parse(get_attr!(attrs, "Weight"));
                        let weighted_pct = AssignmentGradeCalcWeight::parse(get_attr!(attrs, "WeightedPct"));

                        Ok(AssignmentGradeCalc {
                            _type: _type,
                            calculated_mark: calculated_mark,
                            points: points,
                            points_possible: points_possible,
                            weight: weight,
                            weighted_pct: weighted_pct,
                        })
                    }
                    _ => Err(DecodingError::UnexpectedEvent(event))
                }
            }
            _ => Err(DecodingError::UnexpectedEvent(event))
        }
    }
}

#[derive(Clone, Debug)]
pub enum AssignmentGradeCalcWeight {
    Percentage(f64),
    Unparseable(String),
}

impl AssignmentGradeCalcWeight {
    fn parse(weight: &str) -> AssignmentGradeCalcWeight {
        let weight = weight.trim();

        if weight.ends_with('%') {
            let w = weight.trim_right_matches('%');

            f64::from_str(w)
                .map(|i| AssignmentGradeCalcWeight::Percentage(i))
                .unwrap_or(AssignmentGradeCalcWeight::Unparseable(weight.to_string()))
        } else {
            AssignmentGradeCalcWeight::Unparseable(weight.to_string())
        }
    }
}

#[derive(Clone, Debug)]
pub struct Assignment {
    pub _type: String,
    pub gradebook_id: String,
    pub measure: String,
    pub date: NaiveDate,
    pub due_date: NaiveDate,
    pub score: AssignmentScore,
    pub score_type: String,
    pub points: AssignmentPoints,
    pub notes: String,
    pub teacher_id: String,
    pub student_id: String,
    pub has_drop_box: bool,
    pub drop_start_date: NaiveDate,
    pub drop_end_date: NaiveDate,
    pub standards: Vec<Standard>,
}

field_slice_helpers!(Assignment, {
    standards => Standard
});

impl<'a> Pairable<'a, String> for Assignment {
    fn unique_key(&'a self) -> &'a String {
        &self.gradebook_id
    }
}

impl SVUEDecodeable for Assignment {
    fn from_event(event: ReaderEvent, events_iter: &mut Events<&[u8]>) -> DecoderResult<Assignment> {
        match event.clone() {
            ReaderEvent::StartElement { name, attributes, .. } => {
                match name.local_name.as_str() {
                    "Assignment" => {
                        let attrs = attributes_vec_to_map(&attributes);

                        let _type = get_attr_owned!(attrs, "Type");
                        let gradebook_id = get_attr_owned!(attrs, "GradebookID");
                        let measure = get_attr_owned!(attrs, "Measure");
                        let date = parse_date!(attrs, "Date");
                        let due_date = parse_date!(attrs, "DueDate");
                        let score = AssignmentScore::parse(get_attr!(attrs, "Score"));
                        let score_type = get_attr_owned!(attrs, "ScoreType");
                        let points = AssignmentPoints::parse(get_attr!(attrs, "Points"));
                        let notes = get_attr_owned!(attrs, "Notes");
                        let teacher_id = get_attr_owned!(attrs, "TeacherID");
                        let student_id = get_attr_owned!(attrs, "StudentID");
                        let has_drop_box = parse_bool!(attrs, "HasDropBox");
                        let drop_start_date = parse_date!(attrs, "DropStartDate");
                        let drop_end_date = parse_date!(attrs, "DropEndDate");
                        let mut standards = Vec::new();

                        loop {
                            match events_iter.next() {
                                Some(Ok(event)) => {
                                    match event.clone() {
                                        ReaderEvent::StartElement { name, .. } => {
                                            match name.local_name.as_str() {
                                                "Standard" => {
                                                    let s = Standard::from_event(event, events_iter)?;
                                                    standards.push(s);
                                                }
                                                "Standards" => {},
                                                "Resources" => {},
                                                _ => { return Err(DecodingError::UnexpectedEvent(event)); }
                                            }
                                        }
                                        ReaderEvent::EndElement { name, .. } => {
                                            match name.local_name.as_str() {
                                                "Standards" => {
                                                    break;
                                                }
                                                _ => {}
                                            }
                                        }
                                        ReaderEvent::Whitespace(_) => {},
                                        _ => { return Err(DecodingError::UnexpectedEvent(event)); }
                                    }
                                }
                                Some(Err(e)) => { return Err(DecodingError::EventError(e)); }
                                None => { return Err(DecodingError::UnexpectedEnd); }
                            }
                        }

                        Ok(Assignment {
                            _type: _type,
                            gradebook_id: gradebook_id,
                            measure: measure,
                            date: date,
                            due_date: due_date,
                            score: score,
                            score_type: score_type,
                            points: points,
                            notes: notes,
                            teacher_id: teacher_id,
                            student_id: student_id,
                            has_drop_box: has_drop_box,
                            drop_start_date: drop_start_date,
                            drop_end_date: drop_end_date,
                            standards: standards,
                        })
                    }
                    _ => Err(DecodingError::UnexpectedEvent(event))
                }
            }
            _ => Err(DecodingError::UnexpectedEvent(event))
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AssignmentScore {
    NotDue,
    NotForGrading,
    NotGraded,
    Percentage(f64),
    Score(f64, f64),
    // this seems to be equivalent to not graded? not really sure; standards based grading (with
    // svue) is very confusing
    SeeStandards,
    Unparseable(String),
}

impl AssignmentScore {
    fn parse(score: &str) -> AssignmentScore {
        match score {
            "Not Due" => AssignmentScore::NotDue,
            "" => AssignmentScore::NotForGrading,
            "Not Graded" => AssignmentScore::NotGraded,
            "See Standards" => AssignmentScore::SeeStandards,
            _ => {
                // probably a better way to do this than to try two regexes
                let score_regex = Regex::new(r"([\d\.]+)\s*out\s*of\s*([\d\.]+)").unwrap();

                match score_regex.captures(score) {
                    Some(cs) => {
                        let score = f64::from_str(cs.at(1).unwrap()).unwrap();
                        let possible_score = f64::from_str(cs.at(2).unwrap()).unwrap();

                        AssignmentScore::Score(score, possible_score)
                    }
                    None => {
                        let pct_regex = Regex::new(r"^([\d\.]+)\s*(?:\(\))?$").unwrap();
                        let captures = pct_regex.captures(score);

                        if captures.is_some() {
                            let pct = f64::from_str(captures.unwrap().at(1).unwrap()).unwrap();

                            AssignmentScore::Percentage(pct)
                        } else {
                            AssignmentScore::Unparseable(score.to_string())
                        }
                    }
                }

            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AssignmentPoints {
    Ungraded(f64),
    Graded(f64, f64),
    Unparseable(String),
}

impl AssignmentPoints {
    fn parse(points: &str) -> AssignmentPoints {
        if points.contains("Points Possible") {
            let regex = Regex::new(r"([\d\.]+)\s*Points\s*Possible").unwrap();

            match regex.captures(points) {
                Some(cs) => {
                    let possible_points = f64::from_str(cs.at(1).unwrap()).unwrap();

                    AssignmentPoints::Ungraded(possible_points)
                }
                None => AssignmentPoints::Unparseable(points.to_string())
            }
        } else {
            let regex = Regex::new(r"([\d\.]+)\s*/\s*([\d\.]+)").unwrap();

            match regex.captures(points) {
                Some(cs) => {
                    let points_scored = f64::from_str(cs.at(1).unwrap()).unwrap();
                    let possible_points = f64::from_str(cs.at(2).unwrap()).unwrap();

                    AssignmentPoints::Graded(points_scored, possible_points)
                }
                None => AssignmentPoints::Unparseable(points.to_string())
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Standard {
    pub subject: String,
    pub mark: String,
    pub description: String,
    pub proficiency: Option<f64>,
    pub proficiency_max_value: f64,
    pub standard_screen_assignments: Vec<StandardScreenAssignment>,
}

field_slice_helpers!(Standard, {
    standard_screen_assignments => StandardScreenAssignment
});

impl SVUEDecodeable for Standard {
    fn from_event(event: ReaderEvent, events_iter: &mut Events<&[u8]>) -> DecoderResult<Standard> {
        match event.clone() {
            ReaderEvent::StartElement { name, attributes, .. } => {
                match name.local_name.as_str() {
                    "Standard" => {
                        let attrs = attributes_vec_to_map(&attributes);

                        let subject = get_attr_owned!(attrs, "Subject");
                        let mark = get_attr_owned!(attrs, "Mark");
                        let description = get_attr_owned!(attrs, "Description");
                        let proficiency = f64::from_str(get_attr!(attrs, "Proficiency")).ok();
                        let proficiency_max_value = parse_float!(f64, attrs, "ProfciencyMaxValue");
                        let mut standard_screen_assignments = Vec::new();

                        loop {
                            match events_iter.next() {
                                Some(Ok(event)) => {
                                    match event.clone() {
                                        ReaderEvent::StartElement { name, .. } => {
                                            match name.local_name.as_str() {
                                                "StandardScreenAssignment" => {
                                                    let ssa = StandardScreenAssignment::from_event(event, events_iter)?;
                                                    standard_screen_assignments.push(ssa);
                                                }
                                                "StandardScreenAssignments" => {},
                                                _ => { return Err(DecodingError::UnexpectedEvent(event)); }
                                            }
                                        }
                                        ReaderEvent::EndElement { name, .. } => {
                                            match name.local_name.as_str() {
                                                "StandardScreenAssignments" => {
                                                    break;
                                                }
                                                _ => {}
                                            }
                                        }
                                        ReaderEvent::Whitespace(_) => {},
                                        _ => { return Err(DecodingError::UnexpectedEvent(event)); }
                                    }
                                }
                                Some(Err(e)) => { return Err(DecodingError::EventError(e)); }
                                None => { return Err(DecodingError::UnexpectedEnd); }
                            }
                        }

                        Ok(Standard {
                            subject: subject,
                            mark: mark,
                            description: description,
                            proficiency: proficiency,
                            proficiency_max_value: proficiency_max_value,
                            standard_screen_assignments: standard_screen_assignments,
                        })
                    }
                    _ => Err(DecodingError::UnexpectedEvent(event))
                }
            }
            _ => Err(DecodingError::UnexpectedEvent(event))
        }
    }
}

#[derive(Clone, Debug)]
pub struct StandardScreenAssignment {
    pub _type: String,
    pub assignment: String,
    pub due_date: NaiveDate,
    pub mark: String,
    pub proficiency: Option<f64>,
    pub proficiency_max_value: f64,
}

impl SVUEDecodeable for StandardScreenAssignment {
    fn from_event(event: ReaderEvent, _: &mut Events<&[u8]>) -> DecoderResult<StandardScreenAssignment> {
        match event.clone() {
            ReaderEvent::StartElement { name, attributes, .. } => {
                match name.local_name.as_str() {
                    "StandardScreenAssignment" => {
                        let attrs = attributes_vec_to_map(&attributes);

                        let _type = get_attr_owned!(attrs, "Type");
                        let assignment = get_attr_owned!(attrs, "Assignment");
                        let due_date = parse_date!(attrs, "DueDate");
                        let mark = get_attr_owned!(attrs, "Mark");
                        let proficiency = f64::from_str(get_attr!(attrs, "Proficiency")).ok();
                        let proficiency_max_value = parse_float!(f64, attrs, "ProfciencyMaxValue");

                        Ok(StandardScreenAssignment {
                            _type: _type,
                            assignment: assignment,
                            due_date: due_date,
                            mark: mark,
                            proficiency: proficiency,
                            proficiency_max_value: proficiency_max_value,
                        })
                    }
                    _ => Err(DecodingError::UnexpectedEvent(event))
                }
            }
            _ => Err(DecodingError::UnexpectedEvent(event))
        }
    }
}
