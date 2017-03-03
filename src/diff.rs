use std::collections::HashMap;
use std::hash::Hash;
use std::iter::IntoIterator;

use gradebook::*;

use chrono::NaiveDate;

pub trait Pairable<'a, K> {
    fn unique_key(&'a self) -> &'a K;
}

pub trait IntoHashMap<'a, K, V> {
    fn into_hash_map(self) -> HashMap<&'a K, &'a V>;
}

impl<'a, C, K, V> IntoHashMap<'a, K, V> for C
    where C: IntoIterator<Item=&'a V>,
          K: 'a + Eq + Hash,
          V: 'a + Pairable<'a, K> {

    #[inline]
    fn into_hash_map(self) -> HashMap<&'a K, &'a V> {
        self.into_iter().fold(HashMap::new(), |mut acc, v| { acc.insert(v.unique_key(), v); acc })
    }
}

pub trait PairableCollection<'a, C, K, V> {
    fn pair_with(&'a self, new: C) -> Vec<(Option<&'a V>, Option<&'a V>)>;
}

fn pair_values<'a, O, N, K, V>(old: O, new: N) -> Vec<(Option<&'a V>, Option<&'a V>)>
    where O: IntoIterator<Item=&'a V>,
          N: IntoHashMap<'a, K, V> + IntoIterator<Item=&'a V>,
          K: 'a + Eq + Hash,
          V: 'a + Pairable<'a, K> {

    let mut new = new.into_hash_map();
    let mut pairs = old.into_iter().fold(Vec::new(), |mut acc, val| {
        acc.push((Some(val), new.remove(val.unique_key())));
        acc
    });
    let mut new_vals: Vec<(Option<&'a V>, Option<&'a V>)> = new.iter()
        .fold(Vec::new(), |mut acc, (_, val)| {
            acc.push((None, Some(val)));
            acc
        });
    pairs.append(&mut new_vals);
    pairs
}

impl<'a, K, V> PairableCollection<'a, &'a [V], K, V> for [V]
    where K: 'a + Eq + Hash,
          V: 'a + Pairable<'a, K> {

    fn pair_with(&'a self, new: &'a [V]) -> Vec<(Option<&'a V>, Option<&'a V>)> {
        pair_values(self, new)
    }
}

impl<'a, K, V> PairableCollection<'a, &'a Vec<V>, K, V> for Vec<V>
    where K: 'a + Eq + Hash,
          V: 'a + Pairable<'a, K> {

    fn pair_with(&'a self, new: &'a Vec<V>) -> Vec<(Option<&'a V>, Option<&'a V>)> {
        pair_values(self.as_slice(), new.as_slice())
    }
}

#[cfg_attr(feature="serde-serialize", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct Changeset {
    pub old: Gradebook,
    pub new: Gradebook,
    pub changes: Vec<CourseChanges>,
}

impl Changeset {
    pub fn diff(old: &Gradebook, new: &Gradebook) -> Option<Changeset> {
        let pairs = old.courses().pair_with(new.courses());
        let changes = pairs.iter().fold(Vec::new(), |mut acc, &(o, n)| {
            if let Some(ccs) = CourseChanges::diff(o, n) {
                acc.push(ccs);
            }
            acc
        });

        if changes.is_empty() {
            None
        } else {
            Some(Changeset {
                old: old.clone(),
                new: new.clone(),
                changes: changes,
            })
        }
    }
}

#[cfg_attr(feature="serde-serialize", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct CourseChanges {
    pub old: Option<Course>,
    pub new: Option<Course>,
    pub assignment_changes: Option<Vec<AssignmentChanges>>,
    pub changes: Option<Vec<CourseChange>>,
}

#[cfg_attr(feature="serde-serialize", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub enum CourseChange {
    Added,
    Dropped,
    CalculatedGradeChange { old: String, new: String },
    PeriodChange { old: i8, new: i8 },
    StaffChange { old: String, new: String },
    StaffEmailChange { old: String, new: String },
    //we don't have a course title change because we pair courses by their title; if the title
    //changes, rvue assumes it's a different course
}

#[cfg_attr(feature="serde-serialize", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct AssignmentChanges {
    pub old: Option<Assignment>,
    pub new: Option<Assignment>,
    pub changes: Vec<AssignmentChange>,
}

#[cfg_attr(feature="serde-serialize", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub enum AssignmentChange {
    Added,
    DateChange { old: NaiveDate, new: NaiveDate },
    Removed,
    DueDateChange { old: NaiveDate, new: NaiveDate },
    NotesChange { old: String, new: String },
    PointsChange { old: AssignmentPoints, new: AssignmentPoints },
    ScoreChange { old: AssignmentScore, new: AssignmentScore },
    ScoreTypeChange { old: String, new: String },
    TitleChange { old: String, new: String },
}

macro_rules! add_change {
    ( $change_t:tt, $variant:tt, $field:tt, $changes:expr, $old:expr, $new:expr ) => {
        if $old.$field != $new.$field {
            $changes.push($change_t::$variant { old: $old.$field.clone(), new: $new.$field.clone() });
        }
    };
}

macro_rules! diff {
    ( [$( $field:tt: $variant:tt ),+], $change_t:tt, $changes:expr, $old:expr, $new:expr ) => {
        $(
            add_change!($change_t, $variant, $field, $changes, $old, $new);
        )+
    };
}

impl AssignmentChanges {
    fn diff(old: &Assignment, new: &Assignment) -> Option<AssignmentChanges> {
        let mut changes = Vec::new();

        diff!([
            date: DateChange,
            due_date: DueDateChange,
            notes: NotesChange,
            points: PointsChange,
            score: ScoreChange,
            score_type: ScoreTypeChange,
            measure: TitleChange
        ], AssignmentChange, changes, &old, &new);

        if changes.is_empty() {
            None
        } else {
            Some(AssignmentChanges {
                old: Some(old.clone()),
                new: Some(new.clone()),
                changes: changes,
            })
        }
    }
}

impl CourseChanges {
    fn diff(old: Option<&Course>, new: Option<&Course>) -> Option<CourseChanges> {
        if old.is_none() && new.is_none() {
            return None;
        }

        let mut course_changes = CourseChanges {
            old: old.cloned(),
            new: new.cloned(),
            assignment_changes: None,
            changes: None,
        };

        match (old, new) {
            (Some(ref c1), Some(ref c2)) => {
                let mut changes = Vec::new();

                diff!([
                    period: PeriodChange,
                    staff: StaffChange,
                    staff_email: StaffEmailChange
                ], CourseChange, changes, &c1, &c2);

                if let Some(grade_change) = Self::diff_overall_grades(&c1.marks[0], &c2.marks[0]) {
                    changes.push(grade_change);
                }

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
            (Some(_), _) | (None, Some(_)) => {
                course_changes.changes = Some(vec![if old.is_none() {
                    CourseChange::Added
                } else {
                    CourseChange::Dropped
                }])
            }
            (None, None) => { return None; }
        }

        if course_changes.assignment_changes.is_none() && course_changes.changes.is_none() {
            None
        } else {
            Some(course_changes)
        }
    }

    fn diff_overall_grades(old: &Mark, new: &Mark) -> Option<CourseChange> {
        let old_grade = old.calculated_grade();
        let new_grade = new.calculated_grade();

        if old_grade != new_grade {
            Some(CourseChange::CalculatedGradeChange {
                old: old_grade,
                new: new_grade,
            })
        } else {
            None
        }
    }

    fn diff_assignments(old: &Mark, new: &Mark) -> Vec<AssignmentChanges> {
        let pairs = old.assignments().pair_with(new.assignments());
        pairs.iter().fold(Vec::new(), |mut acc, &(o, n)| {
            match (o, n) {
                (Some(ref a1), Some(ref a2)) => {
                    if let Some(acs) = AssignmentChanges::diff(a1, a2) {
                        acc.push(acs);
                    }
                    acc
                }
                (Some(_), _) | (_, Some(_)) => {
                    acc.push(AssignmentChanges {
                        old: o.cloned(),
                        new: n.cloned(),
                        changes: vec![if o.is_none() {
                            AssignmentChange::Added
                        } else {
                            AssignmentChange::Removed
                        }]
                    });
                    acc
                }
                _ => acc
            }
        })
    }
}
