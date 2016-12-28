use std::collections::HashMap;
use std::hash::Hash;

use gradebook::*;

use chrono::NaiveDate;

pub trait Pairable<'a, K> {
    fn unique_key(&'a self) -> &'a K;
}

pub trait PairableCollection<'a, K, V> {
    fn pair_with(&self, new: &'a [V]) -> Vec<(Option<&'a V>, Option<&'a V>)>;
}

impl <'a, K, V> PairableCollection<'a, K, V>
    where K: 'a + Eq + Hash,
          V: Pairable<'a, K> {

    fn pair_values(old: &'a [V], new: &'a [V]) -> Vec<(Option<&'a V>, Option<&'a V>)> {
        let mut new = Self::keyed_values_to_map(new);
        let mut pairs = Vec::new();

        for val in old.iter() {
            match new.remove(val.unique_key()) {
                Some(v) => { pairs.push((Some(val), Some(v))); }
                None => { pairs.push((Some(val), None)); }
            }
        }

        for (_, val) in new.iter() {
            pairs.push((None, Some(val)));
        }

        pairs
    }

    #[inline]
    fn keyed_values_to_map(values: &'a [V]) -> HashMap<&'a K, &'a V> {
        values.iter().fold(HashMap::new(), |mut acc, v| { acc.insert(v.unique_key(), v); acc })
    }
}

impl<'a, K, V> PairableCollection<'a, K, V> for &'a [V]
    where K: 'a + Eq + Hash,
          V: Pairable<'a, K> {

    fn pair_with(&self, new: &'a [V]) -> Vec<(Option<&'a V>, Option<&'a V>)> {
        PairableCollection::pair_values(self, new)
    }
}

#[derive(Clone, Debug)]
pub struct Changeset<'a> {
    pub old: &'a Gradebook,
    pub new: &'a Gradebook,
    pub changes: Vec<CourseChanges<'a>>,
}

impl<'a> Changeset<'a> {
    pub fn diff(old: &'a Gradebook, new: &'a Gradebook) -> Option<Changeset<'a>> {
        let pairs = old.courses().pair_with(new.courses());
        let mut changes = Vec::new();

        for &(o, n) in pairs.iter() {
            match CourseChanges::diff(o, n) {
                Some(ccs) => { changes.push(ccs); }
                None => {}
            }
        }

        if changes.is_empty() {
            None
        } else {
            Some(Changeset {
                old: old,
                new: new,
                changes: changes,
            })
        }
    }
}

#[derive(Clone, Debug)]
pub struct CourseChanges<'a> {
    pub old: Option<&'a Course>,
    pub new: Option<&'a Course>,
    pub assignment_changes: Option<Vec<AssignmentChanges<'a>>>,
    pub changes: Option<Vec<CourseChange<'a>>>,
}

#[derive(Clone, Debug)]
pub enum CourseChange<'a> {
    Added,
    Dropped,
    PeriodChange { old: &'a i8, new: &'a i8 },
    StaffChange { old: &'a str, new: &'a str },
    StaffEmailChange { old: &'a str, new: &'a str },
    //we don't have a course title change because we pair courses by their title; if the title
    //changes, rvue assumes it's a different course
}

#[derive(Clone, Debug)]
pub struct AssignmentChanges<'a> {
    pub old: Option<&'a Assignment>,
    pub new: Option<&'a Assignment>,
    pub changes: Vec<AssignmentChange<'a>>,
}

#[derive(Clone, Debug)]
pub enum AssignmentChange<'a> {
    Added,
    DateChange { old: &'a NaiveDate, new: &'a NaiveDate },
    Removed,
    DueDateChange { old: &'a NaiveDate, new: &'a NaiveDate },
    NotesChange { old: &'a str, new: &'a str },
    PointsChange { old: &'a AssignmentPoints, new: &'a AssignmentPoints },
    ScoreChange { old: &'a AssignmentScore, new: &'a AssignmentScore },
    ScoreTypeChange { old: &'a str, new: &'a str },
    TitleChange { old: &'a str, new: &'a str },
}

macro_rules! add_change {
    ( $change_t:tt, $variant:tt, $field:tt, $changes:expr, $old:expr, $new:expr ) => {
        if $old.$field != $new.$field {
            $changes.push($change_t::$variant { old: &$old.$field, new: &$new.$field });
        }
    };
}

macro_rules! ass_change {
    ( $variant:tt, $field:tt, $changes:expr, $old:expr, $new:expr ) => {
        add_change!(AssignmentChange, $variant, $field, $changes, $old, $new);
    };
}

macro_rules! course_change {
    ( $variant:tt, $field:tt, $changes:expr, $old:expr, $new:expr ) => {
        add_change!(CourseChange, $variant, $field, $changes, $old, $new);
    };
}

impl<'a> AssignmentChanges<'a> {
    fn diff(old: &'a Assignment, new: &'a Assignment) -> Option<AssignmentChanges<'a>> {
        let mut changes = Vec::new();

        ass_change!(DateChange, date, changes, old, new);
        ass_change!(DueDateChange, due_date, changes, old, new);
        ass_change!(NotesChange, notes, changes, old, new);
        ass_change!(PointsChange, points, changes, old, new);
        ass_change!(ScoreChange, score, changes, old, new);
        ass_change!(ScoreTypeChange, score_type, changes, old, new);
        ass_change!(TitleChange, measure, changes, old, new);

        if changes.is_empty() {
            None
        } else {
            Some(AssignmentChanges {
                old: Some(old),
                new: Some(new),
                changes: changes,
            })
        }
    }
}

impl<'a> CourseChanges<'a> {
    fn diff(old: Option<&'a Course>, new: Option<&'a Course>) -> Option<CourseChanges<'a>> {
        if old.is_none() && new.is_none() {
            return None;
        }

        let mut course_changes = CourseChanges {
            old: old,
            new: new,
            assignment_changes: None,
            changes: None,
        };

        match (old, new) {
            (Some(ref c1), Some(ref c2)) => {
                let mut changes = Vec::new();

                course_change!(PeriodChange, period, changes, c1, c2);
                course_change!(StaffChange, staff, changes, c1, c2);
                course_change!(StaffEmailChange, staff_email, changes, c1, c2);

                let assignment_changes = Self::diff_assignments(&c1.marks[0], &c2.marks[0]);

                match (changes.is_empty(), assignment_changes.is_empty()) {
                    (true, true) => { return None; }
                    (true, false) =>  { course_changes.assignment_changes = Some(assignment_changes); }
                    (false, true) => { course_changes.changes = Some(changes); }
                    (false, false) => {
                        course_changes.assignment_changes = Some(assignment_changes);
                        course_changes.changes = Some(changes);
                    }
                }
            }
            (Some(_), None) => { course_changes.changes = Some(vec![CourseChange::Dropped]); }
            (None, Some(_)) => { course_changes.changes = Some(vec![CourseChange::Added]); }
            (None, None) => { return None; }
        }

        if course_changes.assignment_changes.is_none() && course_changes.changes.is_none() {
            None
        } else {
            Some(course_changes)
        }
    }

    fn diff_assignments(old: &'a Mark, new: &'a Mark) -> Vec<AssignmentChanges<'a>> {
        let pairs = old.assignments().pair_with(new.assignments());
        let mut changes = Vec::new();

        for &(o, n) in pairs.iter() {
            match (o, n) {
                (Some(ref a1), Some(ref a2)) => {
                    match AssignmentChanges::diff(a1, a2) {
                        Some(acs) => { changes.push(acs); }
                        None => {}
                    }
                }
                (Some(ref a1), None) => {
                    changes.push(AssignmentChanges {
                        old: Some(a1),
                        new: None,
                        changes: vec![AssignmentChange::Removed],
                    });
                }
                (None, Some(ref a2)) => {
                    changes.push(AssignmentChanges {
                        old: None,
                        new: Some(a2),
                        changes: vec![AssignmentChange::Added],
                    });
                }
                (None, None) => {}
            }
        }

        changes
    }
}
