//! Process-creation flags shared by the places that spawn helper programs.
/// Windows `CREATE_NO_WINDOW`: the child gets no console window.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;
