extern crate image;
extern crate nalgebra as na;

mod candidates;
mod helper;
mod interop;
mod inverse_depth;
mod multires;

use inverse_depth::InverseDepth;
use na::DMatrix;

// #[allow(dead_code)]
fn main() {
    // let evaluations: Vec<_> = (0..40).map(evaluate_icl_image).collect();
    let accum_eval = (0..40).map(evaluate_icl_image).fold(
        [(0.0, 0.0), (0.0, 0.0), (0.0, 0.0)],
        |mut acc, eval| {
            acc[0] = (
                acc[0].0 + eval[0].0,
                acc[0].1 + eval[0].0 * eval[0].1.unwrap_or(0.0),
            );
            acc[1] = (
                acc[1].0 + eval[1].0,
                acc[1].1 + eval[1].0 * eval[1].1.unwrap_or(0.0),
            );
            acc[2] = (
                acc[2].0 + eval[2].0,
                acc[2].1 + eval[2].0 * eval[2].1.unwrap_or(0.0),
            );
            acc
        },
    );
    let mean_eval_dso = (accum_eval[0].0 / 40.0, accum_eval[0].1 / accum_eval[0].0);
    let mean_eval_stat = (accum_eval[1].0 / 40.0, accum_eval[1].1 / accum_eval[1].0);
    let mean_eval_random = (accum_eval[2].0 / 40.0, accum_eval[2].1 / accum_eval[2].0);
    println!("DSO (ratio, rmse): {:?}", mean_eval_dso);
    println!("Stats (ratio, rmse): {:?}", mean_eval_stat);
    println!("Random (ratio, rmse): {:?}", mean_eval_random);
}

// Inverse Depth stuff ###############################################

fn inverse_depth_visual(inverse_mat: &DMatrix<InverseDepth>) -> DMatrix<u8> {
    inverse_mat.map(|idepth| inverse_depth::visual_enum(&idepth))
}

fn evaluate_icl_image(id: usize) -> [(f32, Option<f32>); 3] {
    let img_path = &format!("icl-rgb/{}.png", id);
    let depth_path = &format!("icl-depth/{}.png", id);
    evaluate_image(img_path, depth_path)
}

fn evaluate_image(rgb_path: &str, depth_path: &str) -> [(f32, Option<f32>); 3] {
    // Load a color image and transform into grayscale.
    let img = image::open(rgb_path).expect("Cannot open image").to_luma();
    let img_matrix = interop::matrix_from_image(img);

    // Read 16 bits PNG image.
    let (width, height, buffer_u16) = helper::read_png_16bits(depth_path).unwrap();
    let depth_map: DMatrix<u16> = DMatrix::from_row_slice(height, width, buffer_u16.as_slice());

    // Evaluate strategies on this image.
    evaluate_all_strategies_for(img_matrix, depth_map)
}

fn evaluate_all_strategies_for(
    img_mat: DMatrix<u8>,
    depth_map: DMatrix<u16>,
) -> [(f32, Option<f32>); 3] {
    // Compute pyramid of matrices.
    let multires_img = multires::mean_pyramid(6, img_mat);

    // Compute pyramid of gradients (without first level).
    let multires_gradient_norm = multires::gradients(&multires_img);

    // canditates
    let multires_candidates = candidates::select(&multires_gradient_norm);
    let higher_res_candidate = multires_candidates.last().unwrap();

    // Evaluate strategies.
    let eval_strat =
        |strat: fn(_) -> _| evaluate_strategy_on(&depth_map, higher_res_candidate, strat);
    let dso_eval = eval_strat(inverse_depth::strategy_dso_mean);
    let stat_eval = eval_strat(inverse_depth::strategy_statistically_similar);
    let random_eval = eval_strat(inverse_depth::strategy_random);
    // println!("DSO: (ratio: {}, rmse: {:?})", dso_ratio, dso_rmse);
    // println!("Stats: (ratio: {}, rmse: {:?})", stat_ratio, stat_rmse);
    // println!("Random: (ratio: {}, rmse: {:?})", random_ratio, random_rmse);
    [dso_eval, stat_eval, random_eval]
}

fn evaluate_strategy_on<F>(
    depth_map: &DMatrix<u16>,
    sparse_candidates: &DMatrix<bool>,
    strategy: F,
) -> (f32, Option<f32>)
where
    F: Fn(Vec<(f32, f32)>) -> InverseDepth,
{
    // Compute the pyramid of depths maps from ground truth dataset.
    let multires_depth_map = mean_pyramid_u16(6, depth_map.clone());

    // Create a half resolution depth map to fit resolution of candidates map.
    let half_res_depth = &multires_depth_map[1];

    // Transform depth map into an InverseDepth matrix.
    let inverse_depth_mat = half_res_depth.map(inverse_depth::from_depth);

    // Only keep InverseDepth values corresponding to point candidates.
    // This is to emulate result of back projection of known points into a new keyframe.
    let inverse_depth_candidates =
        sparse_candidates.zip_map(&inverse_depth_mat, |is_candidate, idepth| {
            if is_candidate {
                idepth
            } else {
                InverseDepth::Unknown
            }
        });

    // Create a multires inverse depth map pyramid
    // with same number of levels than the multires image.
    let fuse = |a, b, c, d| inverse_depth::fuse(a, b, c, d, &strategy);
    let multires_inverse_depth = multires::pyramid_with_max_n_levels(
        5,
        inverse_depth_candidates,
        |mat| mat,
        |mat| multires::halve(&mat, fuse),
    );

    // Compare the lowest resolution inverse depth map of the pyramid
    // with the one from ground truth.
    let lower_res_inverse_depth = multires_inverse_depth.last().unwrap();
    let lower_res_inverse_depth_gt = multires_depth_map
        .last()
        .unwrap()
        .map(inverse_depth::from_depth);
    evaluate_idepth(lower_res_inverse_depth, &lower_res_inverse_depth_gt)
}

// Given an InverseDepth matrix and its ground truth, compute:
//   (1) the ratio of available InverseDepth values (not Unknown or Discarded).
//   (2) the MSE for these available predictions if any.
fn evaluate_idepth(
    idepth_map: &DMatrix<InverseDepth>,
    gt: &DMatrix<InverseDepth>,
) -> (f32, Option<f32>) {
    assert_eq!(idepth_map.shape(), gt.shape());
    let (height, width) = gt.shape();
    let mut count = 0;
    let mut mse = 0.0;
    idepth_map
        .iter()
        .zip(gt.iter())
        .for_each(|(idepth, idepth_gt)| match (idepth, idepth_gt) {
            (InverseDepth::WithVariance(x, _), InverseDepth::WithVariance(x_gt, _)) => {
                count = count + 1;
                mse = mse + (x - x_gt) * (x - x_gt);
            }
            _ => {}
        });
    if count == 0 {
        (0.0, None)
    } else {
        (
            count as f32 / (height as f32 * width as f32),
            Some((mse / count as f32).sqrt()),
        )
    }
}

// Helpers ###########################################################

fn mean_pyramid_u16(max_levels: usize, mat: DMatrix<u16>) -> Vec<DMatrix<u16>> {
    multires::pyramid_with_max_n_levels(
        max_levels,
        mat,
        |m| m,
        |m| {
            multires::halve(m, |a, b, c, d| {
                let a = a as u32;
                let b = b as u32;
                let c = c as u32;
                let d = d as u32;
                ((a + b + c + d) / 4) as u16
            })
        },
    )
}
