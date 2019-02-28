use nalgebra as na;
use std::path::PathBuf;

use crate::core::camera::Intrinsics;

pub type Float = f32;

/// U16 depth values are scaled for better precision.
/// So 5000 in the 16 bits gray png corresponds to 1 meter.
pub const DEPTH_SCALE: Float = 5000.0;

/// Intrinsics parameters of the ICL-NUIM dataset.
pub const INTRINSICS_ICL_NUIM: Intrinsics = Intrinsics {
    principal_point: (319.5, 239.5),
    focal_length: 1.0,
    scaling: (481.20, -480.00),
    skew: 0.0,
};

#[derive(Debug)]
pub struct Frame {
    pub timestamp: f64,
    pub pose: na::Isometry3<Float>,
}

#[derive(Debug)]
pub struct Association {
    pub depth_timestamp: f64,
    pub depth_file_path: PathBuf,
    pub color_timestamp: f64,
    pub color_file_path: PathBuf,
}

impl std::string::ToString for Frame {
    fn to_string(&self) -> String {
        let t = self.pose.translation.vector;
        let q = self.pose.rotation.into_inner().coords;
        format!(
            "{} {} {} {} {} {} {} {}",
            self.timestamp, t.x, t.y, t.z, q.x, q.y, q.z, q.w
        )
    }
}

pub mod parse {
    use super::*;
    use nom::{
        alt, anychar, call, do_parse, double, float, is_not, many0, map, named, space, tag,
        types::CompleteStr,
    };

    pub fn associations(file_content: String) -> Result<Vec<Association>, String> {
        multi_line(association_line, file_content)
    }

    pub fn groundtruth(file_content: String) -> Result<Vec<Frame>, String> {
        multi_line(groundtruth_line, file_content)
    }

    fn multi_line<F, T>(line_parser: F, file_content: String) -> Result<Vec<T>, String>
    where
        F: Fn(CompleteStr) -> nom::IResult<CompleteStr, Option<T>>,
    {
        let mut vec_data = Vec::new();
        for line in file_content.lines() {
            match line_parser(CompleteStr(line)) {
                Ok((_, Some(data))) => vec_data.push(data),
                Ok(_) => (),
                Err(_) => return Err("Parsing error".to_string()),
            }
        }
        Ok(vec_data)
    }

    // nom parsers #############################################################

    // Associations --------------------

    // Association line is either a comment or two timestamps and file paths.
    named!(association_line<CompleteStr, Option<Association> >,
        alt!( map!(comment, |_| None) | map!(association, |a| Some(a)) )
    );

    // Parse an association of depth and color timestamps and file paths.
    named!(association<CompleteStr, Association>,
        do_parse!(
            depth_timestamp: double >> space >>
            depth_file_path: path >> space >>
            color_timestamp: double >> space >>
            color_file_path: path >>
            (Association { depth_timestamp, depth_file_path, color_timestamp, color_file_path })
        )
    );

    named!(path<CompleteStr, PathBuf>,
        map!(is_not!(" \t\r\n"), |s| PathBuf::from(*s))
    );

    // Ground truth --------------------

    // Ground truth line is either a comment or a frame timestamp and pose.
    named!(groundtruth_line<CompleteStr, Option<Frame> >,
        alt!( map!(comment, |_| None) | map!(frame, |f| Some(f)) )
    );

    // Parse a comment.
    named!(comment<CompleteStr,()>,
        do_parse!( tag!("#") >> many0!(anychar) >> ())
    );

    // Parse a frame.
    named!(frame<CompleteStr, Frame>,
        do_parse!(
            t: double >> space >>
            p: pose >>
            (Frame { timestamp: t, pose: p })
        )
    );

    // Parse extrinsics camera parameters.
    named!(pose<CompleteStr, na::Isometry3<Float> >,
        do_parse!(
            t: translation >> space >>
            r: rotation >>
            (na::Isometry3::from_parts(t, r))
        )
    );

    // Parse components of a translation.
    named!(translation<CompleteStr, na::Translation3<Float> >,
        do_parse!(
            x: float >> space >>
            y: float >> space >>
            z: float >>
            (na::Translation3::new(x, y, z))
        )
    );

    // Parse components of a unit quaternion describing the rotation.
    named!(rotation<CompleteStr, na::UnitQuaternion<Float> >,
        do_parse!(
            qx: float >> space >>
            qy: float >> space >>
            qz: float >> space >>
            qw: float >>
            (na::UnitQuaternion::from_quaternion(na::Quaternion::new(qw, qx, qy, qz)))
        )
    );

} // pub mod parse
