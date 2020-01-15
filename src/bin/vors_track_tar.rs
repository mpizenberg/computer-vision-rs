// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

extern crate image;
extern crate nalgebra as na;
extern crate visual_odometry_rs as vors;

use na::DMatrix;
use std::{env, error::Error, io::Read, io::Seek, io::SeekFrom, path::PathBuf};

use byteorder::{BigEndian, ReadBytesExt};
use png::HasParameters;
use std::collections::HashMap;
use std::{fs::File, io::Cursor};
use tar;

use vors::core::camera::Intrinsics;
use vors::core::track::inverse_compositional as track;
use vors::dataset::tum_rgbd;
use vors::misc::interop;

fn main() {
    let args: Vec<String> = env::args().collect();
    if let Err(error) = my_run(&args) {
        eprintln!("{:?}", error);
        std::process::exit(1);
    }
}

const USAGE: &str = "Usage: ./vors_track_tar [fr1|fr2|fr3|icl] archive.tar";

fn my_run(args: &[String]) -> Result<(), Box<dyn Error>> {
    // Check that the arguments are correct.
    let valid_args = check_args(args)?;

    // Prepare file entries from the archive.
    let mut archive_file = File::open(&valid_args.archive_path)?;
    let mut archive = tar::Archive::new(&archive_file);
    let mut entries = HashMap::new();
    for file in archive.entries()? {
        // Check for an I/O error.
        let file = file?;
        entries.insert(
            file.header().path()?.to_str().expect("oops").to_owned(),
            FileEntry {
                offset: file.raw_file_position(),
                length: file.header().size()?,
            },
        );
    }

    // Build a vector containing timestamps and full paths of images.
    let associations_buffer = get_buffer("associations.txt", &mut archive_file, &entries)?;
    let associations = parse_associations_buf(associations_buffer.as_slice())?;

    // Setup tracking configuration.
    let config = track::Config {
        nb_levels: 6,
        candidates_diff_threshold: 7,
        depth_scale: tum_rgbd::DEPTH_SCALE,
        intrinsics: valid_args.intrinsics,
        idepth_variance: 0.0001,
    };

    // Initialize tracker with first depth and color image.
    let (depth_map, img) = read_images(&associations[0], &mut archive_file, &entries)?;
    let depth_time = associations[0].depth_timestamp;
    let img_time = associations[0].color_timestamp;
    let mut tracker = config.init(depth_time, &depth_map, img_time, img);

    // Track every frame in the associations file.
    for assoc in associations.iter().skip(1) {
        // Load depth and color images.
        let (depth_map, img) = read_images(assoc, &mut archive_file, &entries)?;

        // Track the rgb-d image.
        tracker.track(
            false,
            assoc.depth_timestamp,
            &depth_map,
            assoc.color_timestamp,
            img,
        );

        // Print to stdout the frame pose.
        let (timestamp, pose) = tracker.current_frame();
        println!("{}", (tum_rgbd::Frame { timestamp, pose }).to_string());
    }

    Ok(())
}

struct Args {
    archive_path: PathBuf,
    intrinsics: Intrinsics,
}

/// Verify that command line arguments are correct.
fn check_args(args: &[String]) -> Result<Args, String> {
    // eprintln!("{:?}", args);
    if let [_, camera_id, archive_path_str] = args {
        let intrinsics = create_camera(camera_id)?;
        let archive_path = PathBuf::from(archive_path_str);
        if archive_path.is_file() {
            Ok(Args {
                intrinsics,
                archive_path,
            })
        } else {
            eprintln!("{}", USAGE);
            Err(format!(
                "The archive does not exist or is not reachable: {}",
                archive_path_str
            ))
        }
    } else {
        eprintln!("{}", USAGE);
        Err("Wrong number of arguments".to_string())
    }
}

/// Create camera depending on `camera_id` command line argument.
fn create_camera(camera_id: &str) -> Result<Intrinsics, String> {
    match camera_id {
        "fr1" => Ok(tum_rgbd::INTRINSICS_FR1),
        "fr2" => Ok(tum_rgbd::INTRINSICS_FR2),
        "fr3" => Ok(tum_rgbd::INTRINSICS_FR3),
        "icl" => Ok(tum_rgbd::INTRINSICS_ICL_NUIM),
        _ => {
            eprintln!("{}", USAGE);
            Err(format!("Unknown camera id: {}", camera_id))
        }
    }
}

/// Open an association file (in bytes form) and parse it into a vector of Association.
fn parse_associations_buf(buffer: &[u8]) -> Result<Vec<tum_rgbd::Association>, Box<dyn Error>> {
    let mut content = String::new();
    let mut slice = buffer;
    slice.read_to_string(&mut content)?;
    tum_rgbd::parse::associations(&content).map_err(|s| s.into())
}

struct FileEntry {
    offset: u64,
    length: u64,
}

fn get_buffer<R: Read + Seek>(
    name: &str,
    file: &mut R,
    entries: &HashMap<String, FileEntry>,
) -> Result<Vec<u8>, std::io::Error> {
    let entry = entries.get(name).expect("Entry is not in archive");
    read_file_entry(entry, file)
}

fn read_file_entry<R: Read + Seek>(
    entry: &FileEntry,
    file: &mut R,
) -> Result<Vec<u8>, std::io::Error> {
    let mut buffer = vec![0; entry.length as usize];
    file.seek(SeekFrom::Start(entry.offset))?;
    file.read_exact(&mut buffer)?;
    Ok(buffer)
}

/// Read a depth and color image given by an association.
fn read_images<R: Read + Seek>(
    assoc: &tum_rgbd::Association,
    file: &mut R,
    entries: &HashMap<String, FileEntry>,
) -> Result<(DMatrix<u16>, DMatrix<u8>), image::ImageError> {
    // Read depth image.
    let depth_path_str = assoc.depth_file_path.to_str().expect("oaea").to_owned();
    let depth_buffer = get_buffer(&depth_path_str, file, entries)?;
    let (w, h, depth_map_vec_u16) = read_png_16bits_buf(depth_buffer.as_slice())?;
    let depth_map = DMatrix::from_row_slice(h, w, depth_map_vec_u16.as_slice());

    // Read color image.
    let img_path_str = assoc.color_file_path.to_str().expect("oaeaauuu").to_owned();
    let img_buffer = get_buffer(&img_path_str, file, entries)?;
    // let img_decoder = image::png::PNGDecoder::new(img_buffer.as_slice())?;
    let img = image::load(Cursor::new(img_buffer), image::ImageFormat::PNG)?;
    let img_mat = interop::matrix_from_image(img.to_luma());

    Ok((depth_map, img_mat))
}

fn read_png_16bits_buf<R: Read>(r: R) -> Result<(usize, usize, Vec<u16>), png::DecodingError> {
    let mut decoder = png::Decoder::new(r);
    // Use the IDENTITY transformation because by default
    // it will use STRIP_16 which only keep 8 bits.
    // See also SWAP_ENDIAN that might be useful
    //   (but seems not possible to use according to documentation).
    decoder.set(png::Transformations::IDENTITY);
    let (info, mut reader) = decoder.read_info()?;
    let mut buffer = vec![0; info.buffer_size()];
    reader.next_frame(&mut buffer)?;

    // Transform buffer into 16 bits slice.
    // if cfg!(target_endian = "big") ...
    let mut buffer_u16 = vec![0; (info.width * info.height) as usize];
    let mut buffer_cursor = Cursor::new(buffer);
    buffer_cursor.read_u16_into::<BigEndian>(&mut buffer_u16)?;

    // Return u16 buffer.
    Ok((info.width as usize, info.height as usize, buffer_u16))
}
