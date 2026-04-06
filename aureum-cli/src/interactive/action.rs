use crate::interactive::field::FieldDecisions;

pub(super) enum Action {
    /// User pressed Enter after deciding each failing field.
    Proceed(FieldDecisions),
    /// User pressed `p` to go back to the previous failing test; carries current partial decisions.
    Previous(FieldDecisions),
    /// User pressed `l` to open the test list; carries current partial decisions.
    ShowList(FieldDecisions),
    Quit,
}

pub(super) enum ListAction {
    JumpTo(usize),
    Quit,
}
