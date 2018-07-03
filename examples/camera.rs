extern crate computer_vision_rs as cv;
extern crate nalgebra;

use cv::camera::{Camera, Extrinsics, Intrinsics};
use nalgebra::{Point2, Quaternion, Translation3, UnitQuaternion};

fn main() {
    // Camera intrinsics.
    let intrinsics = Intrinsics {
        principal_point: (319.5, 239.5),
        focal_length: 1.0,
        scaling: (481.20, -480.00),
        skew: 0.0,
    };

    // Camera at frame 1.
    let translation_1 = Translation3::new(0.0, 0.0, -2.25);
    let rotation_1 = UnitQuaternion::from_quaternion(Quaternion::new(1.0, 0.0, 0.0, 0.0));
    let extrinsics_1 = Extrinsics::new(translation_1, rotation_1);
    let camera_1 = Camera::new(intrinsics.clone(), extrinsics_1);

    // Camera at frame 600.
    let translation_600 = Translation3::new(0.310245, -0.43235, -1.48106);
    let rotation_600 =
        UnitQuaternion::from_quaternion(Quaternion::new(0.933403, 0.0471923, 0.322162, 0.150811));
    let extrinsics_600 = Extrinsics::new(translation_600, rotation_600);
    let camera_600 = Camera::new(intrinsics.clone(), extrinsics_600);

    // Bottom left picture frame corner in frame 1.
    let bl_pos_1 = Point2::new(386.0, 277.0);
    let bl_depth_1 = 16820.0 / 5000.0;

    // Bottom left picture frame corner in frame 600.
    let bl_pos_600 = Point2::new(14.0, 102.0);
    let bl_depth_600 = 10670.0 / 5000.0;

    // Back project the bottom left corner in both frame 1 and 600.
    // We should normally obtain the same 3D point.
    println!(
        "Back Projected 1: {}",
        camera_1.back_project(bl_pos_1, bl_depth_1)
    );
    println!(
        "Back Projected 600: {}",
        camera_600.back_project(bl_pos_600, bl_depth_600)
    );
}
