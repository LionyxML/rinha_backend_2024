#![allow(clippy::inconsistent_digit_grouping)]

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use tokio::sync::Mutex;

#[derive(Debug, Clone)]
struct Client {
    id: i64,
    limite: i64,
    saldo: i64,
    transacoes: Vec<Transaction>,
}

impl Client {
    fn new(id: i64, limite: i64, saldo: i64) -> Self {
        Self {
            id,
            limite,
            saldo,
            transacoes: vec![],
        }
    }

    fn update_saldo(&mut self, new_saldo: i64) {
        self.saldo = new_saldo
    }

    fn add_transacao(&mut self, new_transaction: Transaction) {
        self.transacoes.push(new_transaction)
    }
}

#[derive(Serialize)]
struct TransactionOkResp {
    limite: i64,
    saldo: i64,
}

#[derive(Clone)]
struct ApiState {
    client_list: Vec<Client>,
}

impl ApiState {
    fn new() -> Self {
        let client_list = vec![
            Client::new(1, 1_000__00, 0),
            Client::new(2, 800__00, 0),
            Client::new(3, 10_000__00, 0),
            Client::new(4, 100_000__00, 0),
            Client::new(5, 5_000__00, 0),
        ];

        Self { client_list }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct TransactionRequest {
    valor: i64,
    tipo: String,
    descricao: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct Transaction {
    valor: i64,
    tipo: String,
    descricao: String,
    realizada_em: DateTime<Utc>,
}

#[derive(Serialize)]
struct Error {
    erro: String,
}

async fn client_transaction(
    Path(client_id): Path<String>,
    State(state): State<Arc<Mutex<ApiState>>>,
    Json(payload): Json<TransactionRequest>,
) -> (StatusCode, Result<Json<TransactionOkResp>, Json<Error>>) {
    let mut state = state.lock().await;

    let client_ids = state
        .client_list
        .clone()
        .into_iter()
        .map(|x| x.id)
        .collect::<Vec<_>>();

    match client_ids.contains(&client_id.parse().unwrap()) {
        true => {
            let target_id = client_id.clone().parse::<i64>().unwrap();
            let target_client = state
                .client_list
                .iter_mut()
                .find(|client| client.id == target_id)
                .unwrap();

            let operation = &payload.tipo;
            let value = payload.valor;

            if payload.tipo != "c" && payload.tipo != "d" {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Err(Json(Error {
                        erro: String::from("Tipo inválido. Precisa ser \"c\" ou \"d\""),
                    })),
                );
            }

            if payload.valor <= 0 {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Err(Json(Error {
                        erro: String::from("Valor deve ser positivo e maior do que zero"),
                    })),
                );
            }

            if !(payload.descricao.chars().count() > 0 && payload.descricao.chars().count() <= 10) {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Err(Json(Error {
                        erro: String::from("Descrição precisa estar entre 1 e 10 caracteres."),
                    })),
                );
            }

            let current_balance = target_client.saldo;
            let limit = target_client.limite;

            let future_value = match operation.as_str() {
                "c" => current_balance + value,
                "d" => current_balance - value,
                _ => current_balance, // TODO: processing error could be here... also... this should be an enum
            };

            if future_value < (0 - limit) {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Err(Json(Error {
                        erro: String::from("erro"),
                    })),
                );
            }

            target_client.add_transacao(Transaction {
                valor: payload.valor,
                tipo: payload.tipo,
                descricao: payload.descricao,
                realizada_em: Utc::now(),
            });

            target_client.update_saldo(future_value);

            (
                StatusCode::OK,
                Ok(Json(TransactionOkResp {
                    limite: target_client.limite,
                    saldo: target_client.saldo,
                })),
            )
        }

        false => (
            StatusCode::NOT_FOUND,
            Err(Json(Error {
                erro: String::from("Id inválido"),
            })),
        ),
    }
}

#[derive(Serialize)]
struct ClientBalanceSaldo {
    total: i64,
    data_extrato: DateTime<Utc>,
    limite: i64,
}

#[derive(Serialize)]
struct ClientBalanceResponse {
    saldo: ClientBalanceSaldo,
    ultimas_transacoes: Vec<Transaction>,
}

async fn client_balance(
    Path(client_id): Path<String>,
    State(state): State<Arc<Mutex<ApiState>>>,
) -> (StatusCode, Result<Json<ClientBalanceResponse>, Json<Error>>) {
    let mut state = state.lock().await;

    let client_ids = state
        .client_list
        .clone()
        .into_iter()
        .map(|x| x.id)
        .collect::<Vec<_>>();

    match client_ids.contains(&client_id.parse().unwrap()) {
        true => {
            let target_id = client_id.clone().parse::<i64>().unwrap();
            let target_client = state
                .client_list
                .iter_mut()
                .find(|client| client.id == target_id)
                .unwrap();

            (
                StatusCode::OK,
                Ok(Json(ClientBalanceResponse {
                    saldo: ClientBalanceSaldo {
                        total: target_client.saldo,
                        data_extrato: Utc::now(),
                        limite: target_client.limite,
                    },
                    ultimas_transacoes: target_client
                        .transacoes
                        .iter()
                        .rev()
                        .take(10)
                        .cloned()
                        .collect(),
                })),
            )
        }
        false => (
            StatusCode::NOT_FOUND,
            Err(Json(Error {
                erro: String::from("Id inválido"),
            })),
        ),
    }
}

async fn root() -> impl IntoResponse {
    StatusCode::ACCEPTED
}

#[tokio::main]
async fn main() {
    let state = Arc::new(Mutex::new(ApiState::new()));

    let app = Router::new()
        .route("/", get(root))
        .route("/clientes/:client_id/transacoes", post(client_transaction))
        .route("/clientes/:client_id/extrato", get(client_balance))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:9999").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
