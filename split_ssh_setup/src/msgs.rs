
pub struct Main;
impl Main {
    pub const STATES_DIR_MSG: &str = 
"\n\nDo you want to use a custom directory to store your VM Salt states (it needs to be inside file_roots with no trailing /)?\n\nLeave blank for no otherwise insert the absolute path; If you are unsure leave this blank: ";
    pub const FILES_DIR_MSG: &str = 
"\n\nDo you want to use a custom directory to store your files managed by salt (it needs to be inside file_roots with no trailing /)?\n\n Leave blank for no otherwise insert the absolute path; If you are unsure leave this blank: ";
} 
