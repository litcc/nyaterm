mod helpers;
mod policy;
use compact::CompactTabActionsMenuState;
use helpers::{TabActionsMenuGeometry, TabActionsSubmenuGeometry};
use helpers::{clamp_tab_actions_position, tab_actions_submenu_position};
use policy::{TabActionMenuGroup, TabActionPolicy, TabActionPolicyInput, TabSessionCapability};

mod compact;
mod overlay;
