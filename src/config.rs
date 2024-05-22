use {
    std::{
        convert::{
            TryInto
        },
        io,
        path::{
            Path
        }
    },
    linux_input::{
        AbsoluteAxis,
        AbsoluteAxisBit,
        Bus,
        ForceFeedback,
        Key,
        RelativeAxis
    },
    indexmap::{
        IndexMap
    }
};

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum DeviceKind {
    Keyboard,
    Mouse,
    Gamepad
}

impl DeviceKind {
    fn try_from_str( string: &str ) -> Option< DeviceKind > {
        let kind = match string {
            "keyboard" => DeviceKind::Keyboard,
            "mouse" => DeviceKind::Mouse,
            "gamepad" => DeviceKind::Gamepad,
            _ => return None
        };

        Some( kind )
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum DevicePreset {
    Keyboard,
    Mouse
}

impl DevicePreset {
    fn try_from_str( string: &str ) -> Option< DevicePreset > {
        let preset = match string {
            "keyboard" => DevicePreset::Keyboard,
            "mouse" => DevicePreset::Mouse,
            _ => return None
        };

        Some( preset )
    }
}

fn try_into_bus_value( value: &toml::value::Value ) -> Option< Bus > {
    if let Some( value ) = value.as_str() {
        Bus::try_from_str( value )
    } else if let Some( value ) = value.as_integer() {
        if value >= 0 && value <= 0xFFFF {
            Some( Bus::Other( value as u16 ) )
        } else {
            None
        }
    } else {
        None
    }
}

fn try_into_key_value( value: &toml::value::Value ) -> Option< Key > {
    if let Some( value ) = value.as_str() {
        Key::try_from_str( value )
    } else if let Some( value ) = value.as_integer() {
        if value >= 0 && value <= 0xFFFF {
            Some( Key::Other( value as u16 ) )
        } else {
            None
        }
    } else {
        None
    }
}

fn try_into_rel_value( value: &toml::value::Value ) -> Option< RelativeAxis > {
    if let Some( value ) = value.as_str() {
        RelativeAxis::try_from_str( value )
    } else if let Some( value ) = value.as_integer() {
        if value >= 0 && value <= 0xFFFF {
            Some( RelativeAxis::Other( value as u16 ) )
        } else {
            None
        }
    } else {
        None
    }
}

fn try_into_ff_value( value: &toml::value::Value ) -> Option< ForceFeedback > {
    if let Some( value ) = value.as_str() {
        ForceFeedback::try_from_str( value )
    } else if let Some( value ) = value.as_integer() {
        if value >= 0 && value <= 0xFFFF {
            Some( ForceFeedback::Other( value as u16 ) )
        } else {
            None
        }
    } else {
        None
    }
}

fn try_into_abs_s( string: &str ) -> Option< AbsoluteAxis > {
    if let Some( axis ) = AbsoluteAxis::try_from_str( string ) {
        return Some( axis );
    }

    if !string.is_empty() && string.bytes().all( |byte| byte.is_ascii_digit() ) {
        if let Ok( axis ) = string.parse() {
            return Some( AbsoluteAxis::Other( axis ) );
        } else {
            return None;
        }
    }

    if !string.is_empty() && string.starts_with( "0x" ) {
        if let Ok( axis ) = u16::from_str_radix( &string[ 2.. ], 16 ) {
            return Some( AbsoluteAxis::Other( axis ) );
        } else {
            return None;
        }
    }

    None
}

impl std::fmt::Display for DeviceKind {
    fn fmt( &self, fmt: &mut std::fmt::Formatter ) -> std::fmt::Result {
        let kind = match *self {
            DeviceKind::Keyboard => "keyboard",
            DeviceKind::Mouse => "mouse",
            DeviceKind::Gamepad => "gamepad"
        };
        fmt.write_str( kind )
    }
}

pub struct DeviceFilter {
    pub kind: Option< DeviceKind >,
    pub bus: Option< Bus >,
    pub vendor: Option< u16 >,
    pub not_vendor: Option< u16 >,
    pub product: Option< u16 >,
    pub version: Option< u16 >,
    pub name: Option< String >,
    pub exclusive: bool,
    pub chmod: Option< u16 >
}

pub struct VirtualDevice {
    pub preset: Option< DevicePreset >,
    pub bus: Option< Bus >,
    pub vendor: Option< u16 >,
    pub product: Option< u16 >,
    pub version: Option< u16 >,
    pub name: Option< String >,
    pub chmod: Option< u16 >,
    pub key_bits: Vec< Key >,
    pub rel_bits: Vec< RelativeAxis >,
    pub abs_bits: Vec< AbsoluteAxisBit >,
    pub ff_bits: Vec< ForceFeedback >,
    pub redirect_force_feedback_to: Option< String >
}

pub struct Script {
    pub device: String,
    pub code: String
}

pub struct Config {
    pub device_filters: IndexMap< String, DeviceFilter >,
    pub virtual_devices: IndexMap< String, VirtualDevice >,
    pub scripts: Vec< Script >,
}

impl Config {
    pub fn load_from_file( path: impl AsRef< Path > ) -> Result< Self, io::Error > {
        let data = std::fs::read_to_string( path )?;
        let doc: toml::Value = data.parse().map_err( |error| io::Error::new( io::ErrorKind::InvalidData, error ) )?;

        fn err( error: String ) -> Result< Config, io::Error > {
            Err( io::Error::new( io::ErrorKind::InvalidData, error ) )
        }

        trait OrErr< T > {
            fn or_err( self, cb: impl FnOnce() -> String ) -> Result< T, io::Error >;
        }

        impl< T > OrErr< T > for Option< T > {
            fn or_err( self, cb: impl FnOnce() -> String ) -> Result< T, io::Error > {
                match self {
                    Some( value ) => Ok( value ),
                    None => Err( io::Error::new( io::ErrorKind::InvalidData, cb() ) )
                }
            }
        }

        let mut device_filters = IndexMap::new();
        let mut virtual_devices = IndexMap::new();
        let mut scripts = Vec::new();

        for (toplevel_key, item) in doc.as_table().unwrap().iter() {
            match toplevel_key.as_str() {
                "device-filter" => {
                    let item = item.as_array().or_err( || format!( "\"{}\" is not an array", toplevel_key ) )?;
                    for (nth, item) in item.iter().enumerate() {
                        let item = item.as_table().or_err( || format!( "\"{}.{}\" is not a table", toplevel_key, nth ) )?;

                        let mut internal_name = None;
                        let mut name = None;
                        let mut bus = None;
                        let mut vendor = None;
                        let mut not_vendor = None;
                        let mut product = None;
                        let mut version = None;
                        let mut kind = None;
                        let mut exclusive = None;
                        let mut chmod = None;

                        for (property_name, item) in item.iter() {
                            match property_name.as_str() {
                                "ref" => {
                                    let item = item.as_str().or_err( || format!( "\"{}.{}.{}\" is not a string", toplevel_key, nth, property_name ) )?.to_owned();
                                    internal_name = Some( item );
                                },
                                "name" => {
                                    let item = item.as_str().or_err( || format!( "\"{}.{}.{}\" is not a string", toplevel_key, nth, property_name ) )?.to_owned();
                                    name = Some( item );
                                },
                                "bus" => {
                                    let item = try_into_bus_value( item ).or_err( || format!( "\"{}.{}.{}\" has an invalid value", toplevel_key, nth, property_name ) )?;
                                    bus = Some( item );
                                },
                                "vendor" => {
                                    let item = item.as_integer().or_err( || format!( "\"{}.{}.{}\" is not an integer", toplevel_key, nth, property_name ) )?.to_owned();
                                    let item = item.try_into().ok().or_err( || format!( "\"{}.{}.{}\" is out of range", toplevel_key, nth, property_name ) )?;
                                    vendor = Some( item );
                                },
                                "not_vendor" => {
                                    let item = item.as_integer().or_err( || format!( "\"{}.{}.{}\" is not an integer", toplevel_key, nth, property_name ) )?.to_owned();
                                    let item = item.try_into().ok().or_err( || format!( "\"{}.{}.{}\" is out of range", toplevel_key, nth, property_name ) )?;
                                    not_vendor = Some( item );
                                },
                                "product" => {
                                    let item = item.as_integer().or_err( || format!( "\"{}.{}.{}\" is not an integer", toplevel_key, nth, property_name ) )?.to_owned();
                                    let item = item.try_into().ok().or_err( || format!( "\"{}.{}.{}\" is out of range", toplevel_key, nth, property_name ) )?;
                                    product = Some( item );
                                },
                                "version" => {
                                    let item = item.as_integer().or_err( || format!( "\"{}.{}.{}\" is not an integer", toplevel_key, nth, property_name ) )?.to_owned();
                                    let item = item.try_into().ok().or_err( || format!( "\"{}.{}.{}\" is out of range", toplevel_key, nth, property_name ) )?;
                                    version = Some( item );
                                },
                                "kind" => {
                                    let item = item.as_str().or_err( || format!( "\"{}.{}.{}\" is not a string", toplevel_key, nth, property_name ) )?.to_owned();
                                    let item = DeviceKind::try_from_str( item.as_str() ).or_err( || format!( "key \"{}.{}.{}\" has an invalid value", toplevel_key, nth, property_name ) )?;
                                    kind = Some( item );
                                },
                                "exclusive" => {
                                    let item = item.as_bool().or_err( || format!( "\"{}.{}.{}\" is not a boolean", toplevel_key, nth, property_name ) )?.to_owned();
                                    exclusive = Some( item );
                                },
                                "chmod" => {
                                    let item = item.as_integer().or_err( || format!( "\"{}.{}.{}\" is not an integer", toplevel_key, nth, property_name ) )?.to_owned();
                                    let item = item.try_into().ok().or_err( || format!( "\"{}.{}.{}\" is out of range", toplevel_key, nth, property_name ) )?;
                                    chmod = Some( item );
                                },
                                property_name => {
                                    return err( format!( "unrecognized key: \"{}.{}.{}\"", toplevel_key, nth, property_name ) )
                                }
                            }
                        }

                        let internal_name = if let Some( internal_name ) = internal_name {
                            internal_name
                        } else if let Some( name ) = name.as_ref() {
                            name.to_owned()
                        } else {
                            return err( format!( "\"{}.{}\" is missing an 'ref'", toplevel_key, nth ) )
                        };

                        device_filters.insert( internal_name, DeviceFilter {
                            name,
                            bus,
                            vendor,
                            not_vendor,
                            product,
                            version,
                            kind,
                            exclusive: exclusive.unwrap_or( false ),
                            chmod
                        });
                    }
                },
                "virtual-device" => {
                    let item = item.as_array().or_err( || format!( "\"{}\" is not a table", toplevel_key ) )?;
                    for (nth, item) in item.iter().enumerate() {
                        let item = item.as_table().or_err( || format!( "\"{}.{}\" is not a table", toplevel_key, nth ) )?;
                        let mut internal_name = None;
                        let mut preset = None;
                        let mut name = None;
                        let mut bus = None;
                        let mut vendor = None;
                        let mut product = None;
                        let mut version = None;
                        let mut chmod = None;
                        let mut key_bits = Vec::new();
                        let mut rel_bits = Vec::new();
                        let mut abs_bits = Vec::new();
                        let mut ff_bits = Vec::new();
                        let mut redirect_force_feedback_to = None;
                        for (property_name, item) in item.iter() {
                            match property_name.as_str() {
                                "ref" => {
                                    let item = item.as_str().or_err( || format!( "\"{}.{}.{}\" is not a string", toplevel_key, nth, property_name ) )?.to_owned();
                                    internal_name = Some( item );
                                },
                                "preset" => {
                                    let item = item.as_str().or_err( || format!( "\"{}.{}.{}\" is not a string", toplevel_key, nth, property_name ) )?.to_owned();
                                    let item = DevicePreset::try_from_str( item.as_str() ).or_err( || format!( "key \"{}.{}.{}\" has an invalid value", toplevel_key, nth, property_name ) )?;
                                    preset = Some( item );
                                },
                                "bus" => {
                                    let item = try_into_bus_value( item ).or_err( || format!( "\"{}.{}.{}\" has an invalid value", toplevel_key, nth, property_name ) )?;
                                    bus = Some( item );
                                },
                                "name" => {
                                    let item = item.as_str().or_err( || format!( "\"{}.{}.{}\" is not a string", toplevel_key, nth, property_name ) )?.to_owned();
                                    name = Some( item );
                                },
                                "vendor" => {
                                    let item = item.as_integer().or_err( || format!( "\"{}.{}.{}\" is not an integer", toplevel_key, nth, property_name ) )?.to_owned();
                                    let item = item.try_into().ok().or_err( || format!( "\"{}.{}.{}\" is out of range", toplevel_key, nth, property_name ) )?;
                                    vendor = Some( item );
                                },
                                "product" => {
                                    let item = item.as_integer().or_err( || format!( "\"{}.{}.{}\" is not an integer", toplevel_key, nth, property_name ) )?.to_owned();
                                    let item = item.try_into().ok().or_err( || format!( "\"{}.{}.{}\" is out of range", toplevel_key, nth, property_name ) )?;
                                    product = Some( item );
                                },
                                "version" => {
                                    let item = item.as_integer().or_err( || format!( "\"{}.{}.{}\" is not an integer", toplevel_key, nth, property_name ) )?.to_owned();
                                    let item = item.try_into().ok().or_err( || format!( "\"{}.{}.{}\" is out of range", toplevel_key, nth, property_name ) )?;
                                    version = Some( item );
                                },
                                "chmod" => {
                                    let item = item.as_integer().or_err( || format!( "\"{}.{}.{}\" is not an integer", toplevel_key, nth, property_name ) )?.to_owned();
                                    let item = item.try_into().ok().or_err( || format!( "\"{}.{}.{}\" is out of range", toplevel_key, nth, property_name ) )?;
                                    chmod = Some( item );
                                },
                                "keys" => {
                                    key_bits = Vec::new();
                                    let item = item.as_array().or_err( || format!( "\"{}.{}.{}\" is not an array", toplevel_key, nth, property_name ) )?.to_owned();
                                    for item in item.iter() {
                                        let item = try_into_key_value( item ).or_err( || format!( "\"{}.{}.{}\" has an invalid value: '{}'", toplevel_key, nth, property_name, item ) )?;
                                        key_bits.push( item );
                                    }
                                },
                                "rel" => {
                                    rel_bits = Vec::new();
                                    let item = item.as_array().or_err( || format!( "\"{}.{}.{}\" is not an array", toplevel_key, nth, property_name ) )?.to_owned();
                                    for item in item.iter() {
                                        let item = try_into_rel_value( item ).or_err( || format!( "\"{}.{}.{}\" has an invalid value: '{}'", toplevel_key, nth, property_name, item ) )?;
                                        rel_bits.push( item );
                                    }
                                },
                                "force-feedback" => {
                                    ff_bits = Vec::new();
                                    let item = item.as_array().or_err( || format!( "\"{}.{}.{}\" is not an array", toplevel_key, nth, property_name ) )?.to_owned();
                                    for item in item.iter() {
                                        let item = try_into_ff_value( item ).or_err( || format!( "\"{}.{}.{}\" has an invalid value: '{}'", toplevel_key, nth, property_name, item ) )?;
                                        ff_bits.push( item );
                                    }
                                },
                                "abs" => {
                                    let item = item.as_table().or_err( || format!( "\"{}.{}.{}\" is not a table", toplevel_key, nth, property_name ) )?;
                                    for (axis_name, item) in item.iter() {
                                        let axis = try_into_abs_s( axis_name ).or_err( || format!( "\"{}.{}.{}\" contains an invalid absolute axis name: '{}'", toplevel_key, nth, property_name, axis_name ) )?;
                                        let item = item.as_table().or_err( || format!( "\"{}.{}.{}.{}\" is not a table", toplevel_key, nth, property_name, axis_name ) )?;
                                        let mut initial_value = None;
                                        let mut minimum = None;
                                        let mut maximum = None;
                                        let mut noise_threshold = None;
                                        let mut deadzone = None;
                                        let mut resolution = None;
                                        for (subproperty_name, item) in item.iter() {
                                            match subproperty_name.as_str() {
                                                "initial-value" => {
                                                    let item = item.as_integer().or_err( || format!( "\"{}.{}.{}.{}.{}\" is not an integer", toplevel_key, nth, property_name, axis_name, subproperty_name ) )?.to_owned();
                                                    let item = item.try_into().ok().or_err( || format!( "\"{}.{}.{}.{}.{}\" is out of range", toplevel_key, nth, property_name, axis_name, subproperty_name ) )?;
                                                    initial_value = Some( item );
                                                },
                                                "minimum" | "min" => {
                                                    let item = item.as_integer().or_err( || format!( "\"{}.{}.{}.{}.{}\" is not an integer", toplevel_key, nth, property_name, axis_name, subproperty_name ) )?.to_owned();
                                                    let item = item.try_into().ok().or_err( || format!( "\"{}.{}.{}.{}.{}\" is out of range", toplevel_key, nth, property_name, axis_name, subproperty_name ) )?;
                                                    minimum = Some( item );
                                                },
                                                "maximum" | "max" => {
                                                    let item = item.as_integer().or_err( || format!( "\"{}.{}.{}.{}.{}\" is not an integer", toplevel_key, nth, property_name, axis_name, subproperty_name ) )?.to_owned();
                                                    let item = item.try_into().ok().or_err( || format!( "\"{}.{}.{}.{}.{}\" is out of range", toplevel_key, nth, property_name, axis_name, subproperty_name ) )?;
                                                    maximum = Some( item );
                                                },
                                                "noise-threshold" => {
                                                    let item = item.as_integer().or_err( || format!( "\"{}.{}.{}.{}.{}\" is not an integer", toplevel_key, nth, property_name, axis_name, subproperty_name ) )?.to_owned();
                                                    let item = item.try_into().ok().or_err( || format!( "\"{}.{}.{}.{}.{}\" is out of range", toplevel_key, nth, property_name, axis_name, subproperty_name ) )?;
                                                    noise_threshold = Some( item );
                                                },
                                                "deadzone" => {
                                                    let item = item.as_integer().or_err( || format!( "\"{}.{}.{}.{}.{}\" is not an integer", toplevel_key, nth, property_name, axis_name, subproperty_name ) )?.to_owned();
                                                    let item = item.try_into().ok().or_err( || format!( "\"{}.{}.{}.{}.{}\" is out of range", toplevel_key, nth, property_name, axis_name, subproperty_name ) )?;
                                                    deadzone = Some( item );
                                                },
                                                "resolution" => {
                                                    let item = item.as_integer().or_err( || format!( "\"{}.{}.{}.{}.{}\" is not an integer", toplevel_key, nth, property_name, axis_name, subproperty_name ) )?.to_owned();
                                                    let item = item.try_into().ok().or_err( || format!( "\"{}.{}.{}.{}.{}\" is out of range", toplevel_key, nth, property_name, axis_name, subproperty_name ) )?;
                                                    resolution = Some( item );
                                                },
                                                subproperty_name => {
                                                    return err( format!( "unrecognized key: \"{}.{}.{}.{}.{}\"", toplevel_key, nth, property_name, axis_name, subproperty_name ) )
                                                }
                                            }
                                        }

                                        let minimum = minimum.or_err( || format!( "missing \"{}.{}.{}.{}.minimum\"", toplevel_key, nth, property_name, axis_name ) )?;
                                        let maximum = maximum.or_err( || format!( "missing \"{}.{}.{}.{}.maximum\"", toplevel_key, nth, property_name, axis_name ) )?;

                                        abs_bits.push( AbsoluteAxisBit {
                                            axis,
                                            initial_value: initial_value.unwrap_or( minimum ),
                                            minimum,
                                            maximum,
                                            deadzone: deadzone.unwrap_or( 0 ),
                                            noise_threshold: noise_threshold.unwrap_or( 0 ),
                                            resolution: resolution.unwrap_or( 0 )
                                        })
                                    }
                                },
                                "redirect-force-feedback-to" => {
                                    let item = item.as_str().or_err( || format!( "\"{}.{}.{}\" is not a string", toplevel_key, nth, property_name ) )?.to_owned();
                                    redirect_force_feedback_to = Some( item );
                                },
                                property_name => {
                                    return err( format!( "unrecognized key: \"{}.{}.{}\"", toplevel_key, nth, property_name ) )
                                }
                            }
                        }

                        let internal_name = if let Some( internal_name ) = internal_name {
                            internal_name
                        } else if let Some( name ) = name.as_ref() {
                            name.to_owned()
                        } else {
                            return err( format!( "\"{}.{}\" is missing an 'ref'", toplevel_key, nth ) )
                        };

                        virtual_devices.insert( internal_name, VirtualDevice {
                            preset,
                            name,
                            bus,
                            vendor,
                            product,
                            version,
                            chmod,
                            key_bits,
                            rel_bits,
                            abs_bits,
                            ff_bits,
                            redirect_force_feedback_to
                        });
                    }
                },
                "script" => {
                    let item = item.as_array().or_err( || format!( "\"{}\" is not an array", toplevel_key ) )?;
                    for (nth, item) in item.iter().enumerate() {
                        let item = item.as_table().or_err( || format!( "\"{}.{}\" is not a table", toplevel_key, nth ) )?;

                        let mut device = None;
                        let mut code = None;
                        for (property_name, item) in item.iter() {
                            match property_name.as_str() {
                                "device" => {
                                    let item = item.as_str().or_err( || format!( "\"{}.'{}'.{}\" is not a string", toplevel_key, nth, property_name ) )?.to_owned();
                                    device = Some( item );
                                },
                                "script" => {
                                    let item = item.as_str().or_err( || format!( "\"{}.'{}'.{}\" is not a string", toplevel_key, nth, property_name ) )?.to_owned();
                                    code = Some( item );
                                },
                                property_name => {
                                    return err( format!( "unrecognized key: \"{}.{}.{}\"", toplevel_key, nth, property_name ) )
                                }
                            }
                        }

                        let device = device.or_err( || format!( "missing \"{}.{}.device\"", toplevel_key, nth ) )?;
                        let code = code.or_err( || format!( "missing \"{}.{}.script\"", toplevel_key, nth ) )?;
                        scripts.push( Script {
                            device,
                            code
                        })
                    }
                },
                toplevel_key => return err( format!( "unrecognized key: \"{}\"", toplevel_key ) )
            }
        }

        for script in &scripts {
            if !device_filters.contains_key( &script.device ) {
                return err( format!( "[[script]] refers to a non-existing device filter: \"{}\"", script.device ) );
            }
        }

        for (virtual_device_name, virtual_device) in &virtual_devices {
            if device_filters.contains_key( virtual_device_name ) {
                return err( format!( "same name used as a device filter and a virtual device: \"{}\"", virtual_device_name ) );
            }

            if let Some( ref target ) = virtual_device.redirect_force_feedback_to {
                if !device_filters.contains_key( target ) {
                    return err( format!( "[[virtual-device]]'s 'redirect-force-feedback-to' refers to a non-existing device filter: \"{}\"", target ) );
                }
            }
        }

        Ok( Config {
            device_filters,
            virtual_devices,
            scripts,
        })
    }
}
