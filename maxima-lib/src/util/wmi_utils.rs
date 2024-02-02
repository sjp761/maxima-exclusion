use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[serde(rename = "Win32_OperatingSystem")]
#[serde(rename_all = "PascalCase")]
pub struct Win32OperatingSystem {
    pub serial_number: String,
    pub install_date: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename = "Win32_BIOS")]
#[serde(rename_all = "PascalCase")]
pub struct Win32BIOS {
    pub serial_number: Option<String>,
    pub manufacturer: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename = "Win32_BaseBoard")]
#[serde(rename_all = "PascalCase")]
pub struct Win32BaseBoard {
    pub serial_number: String,
    pub manufacturer: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename = "Win32_VideoController")]
pub struct Win32VideoController {
    #[serde(rename = "PNPDeviceId")]
    pub pnp_device_id: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename = "Win32_DiskDrive")]
pub struct Win32DiskDrive {
    #[serde(rename = "SerialNumber")]
    pub serial_number: String,
}
