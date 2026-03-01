use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use tracing::{debug, instrument};

use crate::domain::{
    DeleteDocumentRequest, DocumentService, DownloadDocumentRequest, GetDocumentRequest,
    LinkDocumentRequest, ListDocumentsByTargetRequest, UnlinkDocumentRequest,
    UpdateDocumentRequest, UploadDocumentRequest,
};
use common::auth::AuthTokenProvider;
use common::grpc::{domain_error_to_status, extract_user_context};
use common::proto::{prost_struct_to_json_value, to_proto_document, to_proto_document_summary};
use ponix_proto_prost::document::v1::{
    self as proto, DeleteDocumentResponse, DownloadDocumentResponse, GetDocumentResponse,
    ListDataStreamDocumentsResponse, ListDefinitionDocumentsResponse,
    ListWorkspaceDocumentsResponse, UnlinkDocumentFromDataStreamResponse,
    UnlinkDocumentFromDefinitionResponse, UnlinkDocumentFromWorkspaceResponse,
    UpdateDataStreamDocumentResponse, UpdateDefinitionDocumentResponse,
    UpdateWorkspaceDocumentResponse, UploadDataStreamDocumentResponse,
    UploadDefinitionDocumentResponse, UploadWorkspaceDocumentResponse,
};
use ponix_proto_tonic::document::v1::tonic::document_service_server::DocumentService as DocumentServiceTrait;

/// 64 KB chunk size for download streaming
const DOWNLOAD_CHUNK_SIZE: usize = 64 * 1024;

/// gRPC handler for DocumentService
pub struct DocumentServiceHandler {
    domain_service: Arc<DocumentService>,
    auth_token_provider: Arc<dyn AuthTokenProvider>,
}

impl DocumentServiceHandler {
    pub fn new(
        domain_service: Arc<DocumentService>,
        auth_token_provider: Arc<dyn AuthTokenProvider>,
    ) -> Self {
        Self {
            domain_service,
            auth_token_provider,
        }
    }
}

#[tonic::async_trait]
impl DocumentServiceTrait for DocumentServiceHandler {
    // --- Standalone operations ---

    #[instrument(name = "GetDocument", skip(self, request), fields(document_id = %request.get_ref().document_id, organization_id = %request.get_ref().organization_id))]
    async fn get_document(
        &self,
        request: Request<proto::GetDocumentRequest>,
    ) -> Result<Response<GetDocumentResponse>, Status> {
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        let document = self
            .domain_service
            .get_document(GetDocumentRequest {
                user_id: user_context.user_id,
                document_id: req.document_id,
                organization_id: req.organization_id,
            })
            .await
            .map_err(domain_error_to_status)?;

        Ok(Response::new(GetDocumentResponse {
            document: Some(to_proto_document(document)),
        }))
    }

    #[instrument(name = "DeleteDocument", skip(self, request), fields(document_id = %request.get_ref().document_id, organization_id = %request.get_ref().organization_id))]
    async fn delete_document(
        &self,
        request: Request<proto::DeleteDocumentRequest>,
    ) -> Result<Response<DeleteDocumentResponse>, Status> {
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        self.domain_service
            .delete_document(DeleteDocumentRequest {
                user_id: user_context.user_id,
                document_id: req.document_id,
                organization_id: req.organization_id,
            })
            .await
            .map_err(domain_error_to_status)?;

        Ok(Response::new(DeleteDocumentResponse {}))
    }

    type DownloadDocumentStream = ReceiverStream<Result<DownloadDocumentResponse, Status>>;

    #[instrument(name = "DownloadDocument", skip(self, request), fields(document_id = %request.get_ref().document_id, organization_id = %request.get_ref().organization_id))]
    async fn download_document(
        &self,
        request: Request<proto::DownloadDocumentRequest>,
    ) -> Result<Response<Self::DownloadDocumentStream>, Status> {
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        let (document, content) = self
            .domain_service
            .download_document(DownloadDocumentRequest {
                user_id: user_context.user_id,
                document_id: req.document_id,
                organization_id: req.organization_id,
            })
            .await
            .map_err(domain_error_to_status)?;

        let (tx, rx) = mpsc::channel(16);

        tokio::spawn(async move {
            // First message: metadata
            let metadata_msg = DownloadDocumentResponse {
                data: Some(proto::download_document_response::Data::Metadata(
                    to_proto_document(document),
                )),
            };
            if tx.send(Ok(metadata_msg)).await.is_err() {
                return;
            }

            // Subsequent messages: content chunks
            for chunk in content.chunks(DOWNLOAD_CHUNK_SIZE) {
                let chunk_msg = DownloadDocumentResponse {
                    data: Some(proto::download_document_response::Data::Chunk(
                        chunk.to_vec().into(),
                    )),
                };
                if tx.send(Ok(chunk_msg)).await.is_err() {
                    return;
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    // --- Data Stream document operations ---

    #[instrument(name = "UploadDataStreamDocument", skip(self, request), fields(data_stream_id = %request.get_ref().data_stream_id, organization_id = %request.get_ref().organization_id))]
    async fn upload_data_stream_document(
        &self,
        request: Request<proto::UploadDataStreamDocumentRequest>,
    ) -> Result<Response<UploadDataStreamDocumentResponse>, Status> {
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        let user_id = user_context.user_id;
        let document = self
            .domain_service
            .upload_document(UploadDocumentRequest {
                user_id: user_id.clone(),
                organization_id: req.organization_id.clone(),
                name: req.name,
                mime_type: req.mime_type,
                metadata: prost_struct_to_json_value(req.metadata),
                content: req.content,
            })
            .await
            .map_err(domain_error_to_status)?;

        self.domain_service
            .link_to_data_stream(LinkDocumentRequest {
                user_id,
                document_id: document.document_id.clone(),
                target_id: req.data_stream_id.clone(),
                organization_id: req.organization_id,
                workspace_id: Some(req.workspace_id),
            })
            .await
            .map_err(domain_error_to_status)?;

        debug!(
            document_id = %document.document_id,
            data_stream_id = %req.data_stream_id,
            "Document uploaded and linked to data stream"
        );

        Ok(Response::new(UploadDataStreamDocumentResponse {
            document: Some(to_proto_document(document)),
        }))
    }

    #[instrument(name = "UpdateDataStreamDocument", skip(self, request), fields(document_id = %request.get_ref().document_id, data_stream_id = %request.get_ref().data_stream_id, organization_id = %request.get_ref().organization_id))]
    async fn update_data_stream_document(
        &self,
        request: Request<proto::UpdateDataStreamDocumentRequest>,
    ) -> Result<Response<UpdateDataStreamDocumentResponse>, Status> {
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        let document = self
            .domain_service
            .update_document(UpdateDocumentRequest {
                user_id: user_context.user_id,
                document_id: req.document_id,
                organization_id: req.organization_id,
                name: req.name,
                mime_type: req.mime_type,
                metadata: req.metadata.map(|s| prost_struct_to_json_value(Some(s))),
                content: req.content,
            })
            .await
            .map_err(domain_error_to_status)?;

        Ok(Response::new(UpdateDataStreamDocumentResponse {
            document: Some(to_proto_document(document)),
        }))
    }

    #[instrument(name = "UnlinkDocumentFromDataStream", skip(self, request), fields(document_id = %request.get_ref().document_id, data_stream_id = %request.get_ref().data_stream_id, organization_id = %request.get_ref().organization_id))]
    async fn unlink_document_from_data_stream(
        &self,
        request: Request<proto::UnlinkDocumentFromDataStreamRequest>,
    ) -> Result<Response<UnlinkDocumentFromDataStreamResponse>, Status> {
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        self.domain_service
            .unlink_from_data_stream(UnlinkDocumentRequest {
                user_id: user_context.user_id,
                document_id: req.document_id,
                target_id: req.data_stream_id,
                organization_id: req.organization_id,
            })
            .await
            .map_err(domain_error_to_status)?;

        Ok(Response::new(UnlinkDocumentFromDataStreamResponse {}))
    }

    #[instrument(name = "ListDataStreamDocuments", skip(self, request), fields(data_stream_id = %request.get_ref().data_stream_id, organization_id = %request.get_ref().organization_id))]
    async fn list_data_stream_documents(
        &self,
        request: Request<proto::ListDataStreamDocumentsRequest>,
    ) -> Result<Response<ListDataStreamDocumentsResponse>, Status> {
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        let documents = self
            .domain_service
            .list_data_stream_documents(ListDocumentsByTargetRequest {
                user_id: user_context.user_id,
                target_id: req.data_stream_id,
                organization_id: req.organization_id,
            })
            .await
            .map_err(domain_error_to_status)?;

        Ok(Response::new(ListDataStreamDocumentsResponse {
            documents: documents
                .into_iter()
                .map(to_proto_document_summary)
                .collect(),
        }))
    }

    // --- Definition document operations ---

    #[instrument(name = "UploadDefinitionDocument", skip(self, request), fields(definition_id = %request.get_ref().definition_id, organization_id = %request.get_ref().organization_id))]
    async fn upload_definition_document(
        &self,
        request: Request<proto::UploadDefinitionDocumentRequest>,
    ) -> Result<Response<UploadDefinitionDocumentResponse>, Status> {
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        let user_id = user_context.user_id;
        let document = self
            .domain_service
            .upload_document(UploadDocumentRequest {
                user_id: user_id.clone(),
                organization_id: req.organization_id.clone(),
                name: req.name,
                mime_type: req.mime_type,
                metadata: prost_struct_to_json_value(req.metadata),
                content: req.content,
            })
            .await
            .map_err(domain_error_to_status)?;

        self.domain_service
            .link_to_definition(LinkDocumentRequest {
                user_id,
                document_id: document.document_id.clone(),
                target_id: req.definition_id.clone(),
                organization_id: req.organization_id,
                workspace_id: None,
            })
            .await
            .map_err(domain_error_to_status)?;

        debug!(
            document_id = %document.document_id,
            definition_id = %req.definition_id,
            "Document uploaded and linked to definition"
        );

        Ok(Response::new(UploadDefinitionDocumentResponse {
            document: Some(to_proto_document(document)),
        }))
    }

    #[instrument(name = "UpdateDefinitionDocument", skip(self, request), fields(document_id = %request.get_ref().document_id, definition_id = %request.get_ref().definition_id, organization_id = %request.get_ref().organization_id))]
    async fn update_definition_document(
        &self,
        request: Request<proto::UpdateDefinitionDocumentRequest>,
    ) -> Result<Response<UpdateDefinitionDocumentResponse>, Status> {
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        let document = self
            .domain_service
            .update_document(UpdateDocumentRequest {
                user_id: user_context.user_id,
                document_id: req.document_id,
                organization_id: req.organization_id,
                name: req.name,
                mime_type: req.mime_type,
                metadata: req.metadata.map(|s| prost_struct_to_json_value(Some(s))),
                content: req.content,
            })
            .await
            .map_err(domain_error_to_status)?;

        Ok(Response::new(UpdateDefinitionDocumentResponse {
            document: Some(to_proto_document(document)),
        }))
    }

    #[instrument(name = "UnlinkDocumentFromDefinition", skip(self, request), fields(document_id = %request.get_ref().document_id, definition_id = %request.get_ref().definition_id, organization_id = %request.get_ref().organization_id))]
    async fn unlink_document_from_definition(
        &self,
        request: Request<proto::UnlinkDocumentFromDefinitionRequest>,
    ) -> Result<Response<UnlinkDocumentFromDefinitionResponse>, Status> {
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        self.domain_service
            .unlink_from_definition(UnlinkDocumentRequest {
                user_id: user_context.user_id,
                document_id: req.document_id,
                target_id: req.definition_id,
                organization_id: req.organization_id,
            })
            .await
            .map_err(domain_error_to_status)?;

        Ok(Response::new(UnlinkDocumentFromDefinitionResponse {}))
    }

    #[instrument(name = "ListDefinitionDocuments", skip(self, request), fields(definition_id = %request.get_ref().definition_id, organization_id = %request.get_ref().organization_id))]
    async fn list_definition_documents(
        &self,
        request: Request<proto::ListDefinitionDocumentsRequest>,
    ) -> Result<Response<ListDefinitionDocumentsResponse>, Status> {
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        let documents = self
            .domain_service
            .list_definition_documents(ListDocumentsByTargetRequest {
                user_id: user_context.user_id,
                target_id: req.definition_id,
                organization_id: req.organization_id,
            })
            .await
            .map_err(domain_error_to_status)?;

        Ok(Response::new(ListDefinitionDocumentsResponse {
            documents: documents
                .into_iter()
                .map(to_proto_document_summary)
                .collect(),
        }))
    }

    // --- Workspace document operations ---

    #[instrument(name = "UploadWorkspaceDocument", skip(self, request), fields(workspace_id = %request.get_ref().workspace_id, organization_id = %request.get_ref().organization_id))]
    async fn upload_workspace_document(
        &self,
        request: Request<proto::UploadWorkspaceDocumentRequest>,
    ) -> Result<Response<UploadWorkspaceDocumentResponse>, Status> {
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        let user_id = user_context.user_id;
        let document = self
            .domain_service
            .upload_document(UploadDocumentRequest {
                user_id: user_id.clone(),
                organization_id: req.organization_id.clone(),
                name: req.name,
                mime_type: req.mime_type,
                metadata: prost_struct_to_json_value(req.metadata),
                content: req.content,
            })
            .await
            .map_err(domain_error_to_status)?;

        self.domain_service
            .link_to_workspace(LinkDocumentRequest {
                user_id,
                document_id: document.document_id.clone(),
                target_id: req.workspace_id.clone(),
                organization_id: req.organization_id,
                workspace_id: None,
            })
            .await
            .map_err(domain_error_to_status)?;

        debug!(
            document_id = %document.document_id,
            workspace_id = %req.workspace_id,
            "Document uploaded and linked to workspace"
        );

        Ok(Response::new(UploadWorkspaceDocumentResponse {
            document: Some(to_proto_document(document)),
        }))
    }

    #[instrument(name = "UpdateWorkspaceDocument", skip(self, request), fields(document_id = %request.get_ref().document_id, workspace_id = %request.get_ref().workspace_id, organization_id = %request.get_ref().organization_id))]
    async fn update_workspace_document(
        &self,
        request: Request<proto::UpdateWorkspaceDocumentRequest>,
    ) -> Result<Response<UpdateWorkspaceDocumentResponse>, Status> {
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        let document = self
            .domain_service
            .update_document(UpdateDocumentRequest {
                user_id: user_context.user_id,
                document_id: req.document_id,
                organization_id: req.organization_id,
                name: req.name,
                mime_type: req.mime_type,
                metadata: req.metadata.map(|s| prost_struct_to_json_value(Some(s))),
                content: req.content,
            })
            .await
            .map_err(domain_error_to_status)?;

        Ok(Response::new(UpdateWorkspaceDocumentResponse {
            document: Some(to_proto_document(document)),
        }))
    }

    #[instrument(name = "UnlinkDocumentFromWorkspace", skip(self, request), fields(document_id = %request.get_ref().document_id, workspace_id = %request.get_ref().workspace_id, organization_id = %request.get_ref().organization_id))]
    async fn unlink_document_from_workspace(
        &self,
        request: Request<proto::UnlinkDocumentFromWorkspaceRequest>,
    ) -> Result<Response<UnlinkDocumentFromWorkspaceResponse>, Status> {
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        self.domain_service
            .unlink_from_workspace(UnlinkDocumentRequest {
                user_id: user_context.user_id,
                document_id: req.document_id,
                target_id: req.workspace_id,
                organization_id: req.organization_id,
            })
            .await
            .map_err(domain_error_to_status)?;

        Ok(Response::new(UnlinkDocumentFromWorkspaceResponse {}))
    }

    #[instrument(name = "ListWorkspaceDocuments", skip(self, request), fields(workspace_id = %request.get_ref().workspace_id, organization_id = %request.get_ref().organization_id))]
    async fn list_workspace_documents(
        &self,
        request: Request<proto::ListWorkspaceDocumentsRequest>,
    ) -> Result<Response<ListWorkspaceDocumentsResponse>, Status> {
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        let documents = self
            .domain_service
            .list_workspace_documents(ListDocumentsByTargetRequest {
                user_id: user_context.user_id,
                target_id: req.workspace_id,
                organization_id: req.organization_id,
            })
            .await
            .map_err(domain_error_to_status)?;

        Ok(Response::new(ListWorkspaceDocumentsResponse {
            documents: documents
                .into_iter()
                .map(to_proto_document_summary)
                .collect(),
        }))
    }
}
