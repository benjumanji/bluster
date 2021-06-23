use dbus::{
    arg::{RefArg, Variant},
    channel::MatchingReceiver,
    message::MatchRule,
    Path,
};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use super::{
    common,
    connection::Connection,
    constants::{LE_ADVERTISEMENT_IFACE, LE_ADVERTISING_MANAGER_IFACE, PATH_BASE},
};
use crate::Error;

#[derive(Clone, Debug)]
struct ServiceData(HashMap<String, Vec<u8>>);

impl ServiceData {
    fn new() -> Self {
        ServiceData(HashMap::new())
    }
}

impl dbus::arg::Arg for ServiceData {
    const ARG_TYPE: dbus::arg::ArgType = dbus::arg::ArgType::Array;

    fn signature() -> dbus::Signature<'static> {
        dbus::Signature::from("a{sv}")
    }
}

impl dbus::arg::RefArg for ServiceData {
    fn arg_type(&self) -> dbus::arg::ArgType {
        <Self as dbus::arg::Arg>::ARG_TYPE
    }

    fn signature(&self) -> dbus::Signature<'static> {
        <Self as dbus::arg::Arg>::signature()
    }

    fn append(&self, iter: &mut dbus::arg::IterAppend) {
        <Self as dbus::arg::Append>::append_by_ref(self, iter);
    }

    fn as_any(&self) -> &dyn std::any::Any where Self: 'static {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any where Self: 'static {
        self
    }
}

impl dbus::arg::Append for ServiceData {
    fn append_by_ref(&self, iter: &mut dbus::arg::IterAppend) {
        let mut to_append = HashMap::new();
        for (k,v) in self.0.iter() {
            let sliced: &[u8] = &*v;
            to_append.insert(&**k, Variant(sliced));
        }
        iter.append(to_append);
    }
}

#[derive(Debug, Clone)]
pub struct Advertisement {
    connection: Arc<Connection>,
    adapter: Path<'static>,
    pub object_path: Path<'static>,
    tree: Arc<Mutex<common::Tree>>,
    is_advertising: Arc<AtomicBool>,
    name: Arc<Mutex<Option<String>>>,
    uuids: Arc<Mutex<Option<Vec<String>>>>,
    service_data: Arc<Mutex<Option<ServiceData>>>,
}

impl Advertisement {
    pub fn new(connection: Arc<Connection>, adapter: Path<'static>) -> Self {
        let mut tree = common::Tree::new();
        let is_advertising = Arc::new(AtomicBool::new(false));
        let is_advertising_release = is_advertising.clone();

        let name = Arc::new(Mutex::new(None));
        let name_property = name.clone();

        let uuids = Arc::new(Mutex::new(None));
        let uuids_property = uuids.clone();

        let service_data = Arc::new(Mutex::new(None));
        let service_data_property = service_data.clone();

        let object_path: Path = format!("{}/advertisement{:04}", PATH_BASE, 0).into();

        let iface_token = tree.register(LE_ADVERTISEMENT_IFACE, |b| {
            b.method_with_cr_async("Release", (), (), move |mut ctx, _cr, ()| {
                is_advertising_release.store(false, Ordering::Relaxed);
                futures::future::ready(ctx.reply(Ok(())))
            });
            b.property("Type")
                .get(|_ctx, _cr| Ok("peripheral".to_owned()));
            b.property("LocalName").get(move |_ctx, _cr| {
                Ok(name_property
                    .lock()
                    .expect("Poisoned mutex")
                    .clone()
                    .unwrap_or_else(String::new))
            });
            b.property("ServiceUUIDs").get(move |_ctx, _cr| {
                Ok(uuids_property
                    .lock()
                    .expect("Poisoned mutex")
                    .clone()
                    .unwrap_or_else(Vec::new))
            });
            b.property("ServiceData").get(move |_ctx, _cr| {
                Ok(service_data_property
                    .lock()
                    .expect("Poisoned mutex")
                    .clone()
                    .unwrap_or_else(ServiceData::new))
            });
        });
        let ifaces = [iface_token, tree.object_manager()];
        tree.insert(object_path.clone(), &ifaces, ());

        let tree = Arc::new(Mutex::new(tree));

        {
            let tree = tree.clone();
            let mut match_rule = MatchRule::new_method_call();
            match_rule.path = Some(object_path.clone());
            connection.default.start_receive(
                match_rule,
                Box::new(move |msg, conn| {
                    tree.lock().unwrap().handle_message(msg, conn).unwrap();
                    true
                }),
            );
        }

        Advertisement {
            connection,
            adapter,
            object_path,
            tree,
            is_advertising,
            name,
            uuids,
            service_data,
        }
    }

    pub fn add_name<T: Into<String>>(self: &Self, name: T) {
        self.name.lock().unwrap().replace(name.into());
    }

    pub fn add_uuids<T: Into<Vec<String>>>(self: &Self, uuids: T) {
        self.uuids.lock().unwrap().replace(uuids.into());
    }

    pub fn add_service_data(
        self: &Self,
        service_uuid: impl Into<String>,
        data: impl Into<Vec<u8>>,
    ) {
        let uuid = service_uuid.into();
        let data = data.into();
        let mut guard = self.service_data.lock().unwrap();
        let m = guard.get_or_insert(ServiceData::new());
        println!("here! {:?}", m);
        m.0.insert(uuid, data);
        println!("there! {:?}", m);
    }

    pub async fn register(self: &Self) -> Result<(), Error> {
        // Register with DBus
        let proxy = self.connection.get_bluez_proxy(&self.adapter);
        proxy
            .method_call(
                LE_ADVERTISING_MANAGER_IFACE,
                "RegisterAdvertisement",
                (
                    &self.object_path,
                    HashMap::<String, Variant<Box<dyn RefArg>>>::new(),
                ),
            )
            .await?;
        self.is_advertising.store(true, Ordering::Relaxed);
        Ok(())
    }

    pub async fn unregister(self: &Self) -> Result<(), Error> {
        let proxy = self.connection.get_bluez_proxy(&self.adapter);

        let method_call = proxy.method_call(
            LE_ADVERTISING_MANAGER_IFACE,
            "UnregisterAdvertisement",
            (&self.object_path,),
        );

        self.is_advertising.store(false, Ordering::Relaxed);

        method_call.await?;
        Ok(())
    }

    pub fn is_advertising(self: &Self) -> bool {
        let is_advertising = self.is_advertising.clone();
        is_advertising.load(Ordering::Relaxed)
    }
}
