use std::convert::TryFrom;
use std::fmt::Debug;

use tonic::transport::Channel;
use tracing::{instrument, trace};

use crate::data::DamlResult;
use crate::data::user::{DamlUser, DamlUserRight};
use crate::grpc_protobuf::com::daml::ledger::api::v2::admin::user_management_service_client::UserManagementServiceClient;
use crate::grpc_protobuf::com::daml::ledger::api::v2::admin::{
    CreateUserRequest, DeleteUserRequest, GetUserRequest, GrantUserRightsRequest, ListUserRightsRequest,
    ListUsersRequest, RevokeUserRightsRequest, Right, UpdateUserIdentityProviderIdRequest, UpdateUserRequest,
};
use crate::service::common::make_request;
use crate::util::Required;

/// Manage participant users and their rights on the v2 Ledger API.
///
/// Users are the unit of authorization: each user holds a set of
/// [`DamlUserRight`]s (act-as, read-as, execute-as, plus participant
/// and IDP admin scopes) that determine what they can do. JWT subject
/// claims address users by id.
///
/// # Authorization
///
/// When the participant requires authentication, all RPCs respond with
/// `UNAUTHENTICATED` if a valid access token is missing, and with
/// `PERMISSION_DENIED` if the token's claims are insufficient.
///
/// Reading or updating "your own" user is always allowed when
/// authenticated as that user; participant admin and identity-provider
/// admin claims open up additional scopes.
#[derive(Debug)]
pub struct DamlUserManagementService<'a> {
    channel: Channel,
    auth_token: Option<&'a str>,
}

/// Listing page returned by [`DamlUserManagementService::list_users`].
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct DamlUserPage {
    pub users: Vec<DamlUser>,
    pub next_page_token: String,
}

impl<'a> DamlUserManagementService<'a> {
    pub fn new(channel: Channel, auth_token: Option<&'a str>) -> Self {
        Self {
            channel,
            auth_token,
        }
    }

    /// Override the JWT token to use for this service.
    pub fn with_token(self, auth_token: &'a str) -> Self {
        Self {
            auth_token: Some(auth_token),
            ..self
        }
    }

    /// Create a new user with an initial set of rights.
    ///
    /// The initial `rights` SHOULD include `CanActAs` /
    /// `CanReadAs(user.primary_party)` so the user can use their
    /// primary party — the server doesn't auto-grant these.
    #[instrument(skip(self, rights))]
    pub async fn create_user(
        &self,
        user: DamlUser,
        rights: impl IntoIterator<Item = DamlUserRight>,
    ) -> DamlResult<DamlUser> {
        let payload = CreateUserRequest {
            user: Some(user.into()),
            rights: rights.into_iter().map(Right::from).collect(),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response = self.client().create_user(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        Ok(DamlUser::from(response.user.req()?))
    }

    /// Fetch a user by id. An empty `user_id` resolves to the
    /// authenticated user (useful for "who am I?" queries).
    #[instrument(skip(self))]
    pub async fn get_user(
        &self,
        user_id: impl Into<String> + Debug,
        identity_provider_id: impl Into<String> + Debug,
    ) -> DamlResult<DamlUser> {
        let payload = GetUserRequest {
            user_id: user_id.into(),
            identity_provider_id: identity_provider_id.into(),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response = self.client().get_user(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        Ok(DamlUser::from(response.user.req()?))
    }

    /// Update modifiable fields of a user.
    ///
    /// `update_paths` is the FieldMask: only the named paths are
    /// touched. Modifiable paths today are `primary_party`,
    /// `is_deactivated`, `primary_party_authentication`, and `metadata`
    /// (and its sub-paths). The user's `id` and
    /// `metadata.resource_version` may also appear, but only to
    /// identify the resource and assert optimistic-concurrency.
    ///
    /// To change a user's rights, use
    /// [`grant_user_rights`](Self::grant_user_rights) /
    /// [`revoke_user_rights`](Self::revoke_user_rights) — those RPCs do
    /// not bump the user's resource version. To change a user's IDP,
    /// use [`update_user_identity_provider_id`](Self::update_user_identity_provider_id).
    #[instrument(skip(self, update_paths))]
    pub async fn update_user(
        &self,
        user: DamlUser,
        update_paths: impl IntoIterator<Item = String>,
    ) -> DamlResult<DamlUser> {
        let payload = UpdateUserRequest {
            user: Some(user.into()),
            update_mask: Some(prost_types::FieldMask {
                paths: update_paths.into_iter().collect(),
            }),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response = self.client().update_user(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        Ok(DamlUser::from(response.user.req()?))
    }

    /// Delete a user. Also drops every right granted to that user.
    #[instrument(skip(self))]
    pub async fn delete_user(
        &self,
        user_id: impl Into<String> + Debug,
        identity_provider_id: impl Into<String> + Debug,
    ) -> DamlResult<()> {
        let payload = DeleteUserRequest {
            user_id: user_id.into(),
            identity_provider_id: identity_provider_id.into(),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        self.client().delete_user(make_request(payload, self.auth_token)?).await?;
        Ok(())
    }

    /// List participant users, paginated.
    ///
    /// `page_size = 0` lets the server pick (see
    /// `UserManagementFeature::max_users_page_size` from
    /// `VersionService.GetLedgerApiVersion`). Empty `page_token` for
    /// the first call; loop until the returned token is empty.
    #[instrument(skip(self))]
    pub async fn list_users(
        &self,
        page_token: impl Into<String> + Debug,
        page_size: i32,
        identity_provider_id: impl Into<String> + Debug,
    ) -> DamlResult<DamlUserPage> {
        let payload = ListUsersRequest {
            page_token: page_token.into(),
            page_size,
            identity_provider_id: identity_provider_id.into(),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response = self.client().list_users(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        Ok(DamlUserPage {
            users: response.users.into_iter().map(DamlUser::from).collect(),
            next_page_token: response.next_page_token,
        })
    }

    /// Add rights to a user. The response carries the rights that were
    /// *newly* granted — already-held rights are no-ops and absent from
    /// the response.
    #[instrument(skip(self, rights))]
    pub async fn grant_user_rights(
        &self,
        user_id: impl Into<String> + Debug,
        rights: impl IntoIterator<Item = DamlUserRight>,
        identity_provider_id: impl Into<String> + Debug,
    ) -> DamlResult<Vec<DamlUserRight>> {
        let payload = GrantUserRightsRequest {
            user_id: user_id.into(),
            rights: rights.into_iter().map(Right::from).collect(),
            identity_provider_id: identity_provider_id.into(),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response = self.client().grant_user_rights(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        response.newly_granted_rights.into_iter().map(DamlUserRight::try_from).collect()
    }

    /// Revoke rights from a user. The response carries the rights that
    /// were *actually* revoked — rights the user didn't hold are no-ops
    /// and absent from the response.
    #[instrument(skip(self, rights))]
    pub async fn revoke_user_rights(
        &self,
        user_id: impl Into<String> + Debug,
        rights: impl IntoIterator<Item = DamlUserRight>,
        identity_provider_id: impl Into<String> + Debug,
    ) -> DamlResult<Vec<DamlUserRight>> {
        let payload = RevokeUserRightsRequest {
            user_id: user_id.into(),
            rights: rights.into_iter().map(Right::from).collect(),
            identity_provider_id: identity_provider_id.into(),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response = self.client().revoke_user_rights(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        response.newly_revoked_rights.into_iter().map(DamlUserRight::try_from).collect()
    }

    /// List every right currently granted to a user. An empty
    /// `user_id` resolves to the authenticated user.
    #[instrument(skip(self))]
    pub async fn list_user_rights(
        &self,
        user_id: impl Into<String> + Debug,
        identity_provider_id: impl Into<String> + Debug,
    ) -> DamlResult<Vec<DamlUserRight>> {
        let payload = ListUserRightsRequest {
            user_id: user_id.into(),
            identity_provider_id: identity_provider_id.into(),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        let response = self.client().list_user_rights(make_request(payload, self.auth_token)?).await?.into_inner();
        trace!(?response);
        response.rights.into_iter().map(DamlUserRight::try_from).collect()
    }

    /// Reassign a user to a different identity provider.
    /// Empty strings select the default IDP for source/target.
    #[instrument(skip(self))]
    pub async fn update_user_identity_provider_id(
        &self,
        user_id: impl Into<String> + Debug,
        source_identity_provider_id: impl Into<String> + Debug,
        target_identity_provider_id: impl Into<String> + Debug,
    ) -> DamlResult<()> {
        let payload = UpdateUserIdentityProviderIdRequest {
            user_id: user_id.into(),
            source_identity_provider_id: source_identity_provider_id.into(),
            target_identity_provider_id: target_identity_provider_id.into(),
        };
        trace!(payload = ?payload, token = ?self.auth_token);
        self.client().update_user_identity_provider_id(make_request(payload, self.auth_token)?).await?;
        Ok(())
    }

    fn client(&self) -> UserManagementServiceClient<Channel> {
        UserManagementServiceClient::new(self.channel.clone())
    }
}
