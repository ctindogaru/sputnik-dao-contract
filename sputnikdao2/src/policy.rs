use std::collections::{HashMap, HashSet};

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{env, AccountId, Balance};
use regex::Regex;

use crate::proposals::{Proposal, ProposalKind, ProposalStatus, Vote};
use crate::types::Action;
use near_sdk::json_types::{WrappedDuration, U128};

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "near_sdk::serde")]
pub enum RoleKind {
    /// Matches everyone, who is not matched by other roles.
    Everyone,
    /// Member: has non zero balance on this DAOs' token.
    Member,
    /// Member with at least given balance (must be non 0).
    MemberBalance(Balance),
    /// Set of accounts.
    Group(Vec<AccountId>),
    /// Set of accounts matches by regex.
    Regex(String),
}

impl RoleKind {
    /// Checks if user matches given role.
    pub fn match_user(&self, user: &UserInfo) -> bool {
        match self {
            RoleKind::Everyone => true,
            RoleKind::Member => user.amount.is_some(),
            RoleKind::MemberBalance(amount) => user.amount.unwrap_or_default() >= *amount,
            RoleKind::Group(accounts) => accounts.contains(&user.account_id),
            RoleKind::Regex(regex) => Regex::new(regex)
                .expect("Invalid regex")
                .is_match(&user.account_id),
        }
    }

    /// Returns the number of people in the this role or None if not supported role kind.
    pub fn get_role_size(&self) -> Option<usize> {
        match self {
            RoleKind::Group(accounts) => Some(accounts.len()),
            _ => None,
        }
    }
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct RolePermission {
    name: String,
    kind: RoleKind,
    /// Set of actions on which proposals that this role is allowed to execute.
    /// <proposal_kind>:<action>
    permissions: HashSet<String>,
}

pub struct UserInfo {
    pub account_id: AccountId,
    pub amount: Option<Balance>,
}

/// Direct weight or ratio to total weight, used for the voting policy.
#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "near_sdk::serde")]
#[serde(untagged)]
pub enum WeightOrRatio {
    Weight(U128),
    Ratio(u64, u64),
}

impl WeightOrRatio {
    /// Convert weight or ratio to specific weight given total weight.
    pub fn to_weight(&self, total_weight: Balance) -> Balance {
        match self {
            WeightOrRatio::Weight(weight) => weight.0,
            WeightOrRatio::Ratio(nom, denom) => (*nom as u128 * total_weight) / *denom as u128,
        }
    }
}

/// How the voting policy votes get weigthed.
#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "near_sdk::serde")]
#[serde(untagged)]
pub enum WeightKind {
    /// Using token amounts and total supply.
    TokenWeight,
    /// Weight of the group role. Roles that don't have scoped group are not supported.
    RoleWeight(String),
}

/// Defines configuration of the vote.
#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct VotePolicy {
    /// Kind of weight to use for votes.
    pub weight_kind: WeightKind,
    /// How many votes to pass this vote.
    pub threshold: WeightOrRatio,
}

impl Default for VotePolicy {
    fn default() -> Self {
        VotePolicy {
            weight_kind: WeightKind::RoleWeight("council".to_string()),
            threshold: WeightOrRatio::Ratio(1, 2),
        }
    }
}

/// Defines voting / decision making policy of this DAO.
#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct Policy {
    /// List of roles and permissions for them in the current policy.
    pub roles: Vec<RolePermission>,
    /// Default vote policy. Used when given proposal kind doesn't have special policy.
    pub default_vote_policy: VotePolicy,
    /// For each proposal kind, defines voting policy.
    pub vote_policy: HashMap<String, VotePolicy>,
    /// Bond for claiming a bounty.
    pub bounty_bond: U128,
    /// Period in which giving up on bounty is not punished.
    pub bounty_forgiveness_period: WrappedDuration,
}

impl Default for Policy {
    /// Defines default policy:
    ///     - everyone can add proposals
    ///     - group consisting of the call can do all actions, consists of caller.
    ///     - non token weighted voting, requires 1/2 of the group to vote
    ///     - bounty bond is 1N
    ///     - bounty forgiveness period is 1 day
    fn default() -> Self {
        Self {
            roles: vec![
                RolePermission {
                    name: "all".to_string(),
                    kind: RoleKind::Everyone,
                    permissions: vec!["*:add_proposal".to_string()].into_iter().collect(),
                },
                RolePermission {
                    name: "council".to_string(),
                    kind: RoleKind::Group(vec![env::predecessor_account_id()]),
                    permissions: vec!["*:*".to_string()].into_iter().collect(),
                },
            ],
            default_vote_policy: VotePolicy::default(),
            vote_policy: HashMap::default(),
            bounty_bond: U128(10u128.pow(24)),
            bounty_forgiveness_period: WrappedDuration::from(1_000_000_000 * 60 * 60 * 24),
        }
    }
}

impl Policy {
    /// Returns set of permissions for given user across all the roles it's member of.
    fn get_user_permissions(&self, user: UserInfo) -> HashSet<String> {
        let mut result = HashSet::default();
        for role in self.roles.iter() {
            if role.kind.match_user(&user) {
                result = result.union(&role.permissions).cloned().collect();
            }
        }
        result
    }

    /// Can given user execute given action on this proposal.
    pub fn can_execute_action(
        &self,
        user: UserInfo,
        proposal_kind: &ProposalKind,
        action: &Action,
    ) -> bool {
        let permissions = self.get_user_permissions(user);
        permissions.contains(&format!(
            "{}:{}",
            proposal_kind.to_policy_label(),
            action.to_policy_label()
        )) || permissions.contains(&format!("{}:*", proposal_kind.to_policy_label()))
            || permissions.contains(&format!("*:{}", action.to_policy_label()))
            || permissions.contains("*:*")
    }

    /// Returns if given proposal kind is token weighted.
    pub fn is_token_weighted(&self, proposal_kind: &ProposalKind) -> bool {
        match self
            .vote_policy
            .get(&proposal_kind.to_policy_label().to_string())
            .unwrap_or(&self.default_vote_policy)
            .weight_kind
        {
            WeightKind::TokenWeight => true,
            _ => false,
        }
    }

    fn internal_get_role(&self, name: &String) -> Option<RolePermission> {
        for role in self.roles.iter() {
            if role.name == *name {
                return Some(role.clone());
            }
        }
        None
    }

    /// Get proposal status for given proposal.
    /// Usually is called after changing it's state.
    pub fn proposal_status(&self, proposal: &Proposal, total_supply: Balance) -> ProposalStatus {
        assert_eq!(
            proposal.status,
            ProposalStatus::InProgress,
            "ERR_PROPOSAL_NOT_IN_PROGRESS"
        );
        let vote_policy = self
            .vote_policy
            .get(&proposal.kind.to_policy_label().to_string())
            .unwrap_or(&self.default_vote_policy);
        let threshold = match &vote_policy.weight_kind {
            WeightKind::TokenWeight => vote_policy.threshold.to_weight(total_supply),
            WeightKind::RoleWeight(role) => {
                self.internal_get_role(role)
                    .expect("ERR_MISSING_ROLE")
                    .kind
                    .get_role_size()
                    .expect("ERR_UNSUPPORTED_ROLE") as Balance
            }
        };
        // Check if there is anything voted above the threshold specificed by policy.
        if proposal.vote_counts[Vote::Approve as usize] >= threshold {
            ProposalStatus::Approved
        } else if proposal.vote_counts[Vote::Reject as usize] >= threshold {
            ProposalStatus::Rejected
        } else if proposal.vote_counts[Vote::Remove as usize] >= threshold {
            ProposalStatus::Removed
        } else {
            proposal.status.clone()
        }
    }
}
