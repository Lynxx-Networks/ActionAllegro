use serde::Serialize;
use serde::Deserialize;
use crate::helpers::{get_actions, push_repo, get_repo, get_workflow_details};
use std::path::Path;
use std::error::Error;
use git2::{Repository, Remote, FetchOptions};

#[cfg(test)]
mod tests {
    use super::*;
    use mockall::{automock, predicate::{self, *}};
    

    #[test]
    fn verify_two_plus_two() {
        assert_eq!(2 + 2, 4);
    }

    #[test]
    fn test_get_workflow_details_with_valid_id() {
        let repo = "owner/repo";
        let token = "your_token";
        let workflow_id = Some(12345);

        let result = get_workflow_details(repo, token, &workflow_id);

        assert!(result.is_ok());
        let json = result.unwrap();
        // Add assertions for the expected JSON response
    }

    #[test]
    fn test_get_workflow_details_with_none_id() {
        let repo = "owner/repo";
        let token = "your_token";
        let workflow_id = None;

        let result = get_workflow_details(repo, token, &workflow_id);

        assert!(result.is_err());
        // Add assertions for the expected error message or behavior
    }

    #[test]
    fn test_get_repo_with_no_path() {
        let repo_slug = "owner/repo";
        let api_key = "your_api_key";
        let path = None;

        let result = get_repo(repo_slug, api_key, &path);

        assert!(result.is_err());
        // Add assertions for the expected error message or behavior when no path is provided
    }

    #[test]
    fn test_get_actions() {
        let repo = "torvalds/linux";
        let token = "your_token";

        let result = get_actions(repo, token);

        assert!(result.is_ok());
        let actions = result.unwrap();
        // Add assertions for the expected actions HashMap
    }


}
