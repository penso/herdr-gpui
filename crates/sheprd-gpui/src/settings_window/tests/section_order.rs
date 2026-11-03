//! The category list starts with General on its own, then runs group by
//! group, each group's pages together and the groups in their own order, so a
//! new section has one obvious place to go: the end of its group.
use super::*;

#[test]
fn sections_run_group_by_group_after_ungrouped_general() {
    // General leads the list outside every group, as the catch-all people
    // look in when they do not know where a setting lives.
    assert_eq!(Section::ALL.first(), Some(&Section::General));
    assert_eq!(Section::General.group(), None);
    assert_eq!(Section::General.nav_item(), 0);
    let groups: Vec<_> = Section::ALL.iter().map(|section| section.group()).collect();
    let mut grouped = groups.clone();
    grouped.sort_by_key(|group| {
        group.map(|group| Group::ALL.iter().position(|candidate| *candidate == group))
    });
    assert_eq!(groups, grouped);
}
