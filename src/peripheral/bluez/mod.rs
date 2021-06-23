mod adapter;
mod advertisement;
mod common;
mod connection;
mod constants;
mod error;
mod gatt;

use std::{collections::HashMap, string::ToString, sync::Arc};
use uuid::Uuid;

use self::{adapter::Adapter, advertisement::Advertisement, connection::Connection, gatt::Gatt};
use crate::{gatt::service::Service, Error};

#[derive(Debug)]
pub struct Peripheral {
    adapter: Adapter,
    gatt: Gatt,
    advertisement: Advertisement,
}

impl Peripheral {
    #[allow(clippy::new_ret_no_self)]
    pub async fn new() -> Result<Self, Error> {
        let connection = Arc::new(Connection::new()?);
        let adapter = Adapter::new(connection.clone()).await?;
        adapter.powered(true).await?;
        let gatt = Gatt::new(connection.clone(), adapter.object_path.clone());
        let advertisement = Advertisement::new(connection, adapter.object_path.clone());

        Ok(Peripheral {
            adapter,
            gatt,
            advertisement,
        })
    }

    pub async fn get_alias(&self) -> Result<String, Error> {
        self.adapter.get_alias().await
    }

    pub async fn set_alias(&self, alias: &str) -> Result<(), Error> {
        self.adapter.set_alias(alias).await
    }

    pub async fn is_powered(self: &Self) -> Result<bool, Error> {
        self.adapter.is_powered().await
    }

    pub async fn register_gatt(&self) -> Result<(), Error> {
        self.gatt.register().await
    }

    pub async fn unregister_gatt(&self) -> Result<(), Error> {
        self.gatt.unregister().await
    }

    pub async fn start_advertising(self: &Self, name: &str, uuids: &[Uuid], data: HashMap<impl ToString, Vec<u8>>) -> Result<(), Error> {
        self.advertisement.add_name(name);
        self.advertisement.add_uuids(
            uuids
                .to_vec()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<String>>(),
        );
        
        for (k,v) in data.into_iter() {
            self.advertisement.add_service_data(k.to_string(), v);
        }

        self.advertisement.register().await
    }

    pub async fn stop_advertising(self: &Self) -> Result<(), Error> {
        self.advertisement.unregister().await
    }

    pub async fn is_advertising(self: &Self) -> Result<bool, Error> {
        Ok(self.advertisement.is_advertising())
    }

    pub fn add_service(self: &Self, service: &Service) -> Result<(), Error> {
        self.gatt.add_service(service)
    }
}
