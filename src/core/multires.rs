//! Helper functions for generation of multi-resolution data.

use nalgebra::{DMatrix, Scalar};

use crate::core::gradient;

/// Recursively generate a pyramid of matrices where each following level
/// is half the previous resolution, computed with the mean of each 2x2 block.
///
/// It consumes the original matrix to keep it as first level of the pyramid without copy.
/// If you need it later, simply use something like `pyramid[0]`.
///
/// PS: since we are using 2x2 blocs,
/// border information is lost for odd resolutions.
/// Some precision is also left to keep the pyramid data as `u8`.
pub fn mean_pyramid(max_levels: usize, mat: DMatrix<u8>) -> Vec<DMatrix<u8>> {
    limited_sequence(
        max_levels,
        mat,
        |m| m,
        |m| {
            halve(m, |a, b, c, d| {
                let a = a as u16;
                let b = b as u16;
                let c = c as u16;
                let d = d as u16;
                ((a + b + c + d) / 4) as u8
            })
        },
    )
}

/// Recursively apply a function transforming an image
/// until it's not possible anymore or the max length is reached.
///
/// The sequence is initialized with the `init` function.
/// Then `f` is applied to each successive result.
///
/// Using `max_length = 0` has the same effect than `max_length = 1` since
/// the result vector contains always at least one matrix (the init matrix).
pub fn limited_sequence<I, F, T, U>(
    max_length: usize,
    mat: DMatrix<T>,
    init: I,
    f: F,
) -> Vec<DMatrix<U>>
where
    I: Fn(DMatrix<T>) -> DMatrix<U>,
    F: Fn(&DMatrix<U>) -> Option<DMatrix<U>>,
    T: Scalar,
    U: Scalar,
{
    let mut length = 1;
    let f_limited = |x: &DMatrix<U>| {
        if length < max_length {
            length = length + 1;
            f(x)
        } else {
            None
        }
    };
    sequence(mat, init, f_limited)
}

/// Recursively apply a function transforming an image
/// until it's not possible anymore.
///
/// The sequence is initialized with the `init` function.
/// Then `f` is applied to each successive result.
pub fn sequence<I, F, T, U>(mat: DMatrix<T>, init: I, mut f: F) -> Vec<DMatrix<U>>
where
    I: Fn(DMatrix<T>) -> DMatrix<U>,
    F: FnMut(&DMatrix<U>) -> Option<DMatrix<U>>,
    T: Scalar,
    U: Scalar,
{
    let mut seq = Vec::new();
    seq.push(init(mat));
    while let Some(new_mat) = f(seq.last().unwrap()) {
        seq.push(new_mat);
    }
    seq
}

/// Halve the resolution of a matrix by applying a function to each 2x2 block.
///
/// If one size of the matrix is < 2 then this function returns None.
/// If one size is odd, its last line/column is dropped.
pub fn halve<F, T, U>(mat: &DMatrix<T>, f: F) -> Option<DMatrix<U>>
where
    F: Fn(T, T, T, T) -> U,
    T: Scalar,
    U: Scalar,
{
    let (r, c) = mat.shape();
    let half_r = r / 2;
    let half_c = c / 2;
    if half_r == 0 || half_c == 0 {
        None
    } else {
        let half_mat = DMatrix::<U>::from_fn(half_r, half_c, |i, j| {
            let a = mat[(2 * i, 2 * j)];
            let b = mat[(2 * i + 1, 2 * j)];
            let c = mat[(2 * i, 2 * j + 1)];
            let d = mat[(2 * i + 1, 2 * j + 1)];
            f(a, b, c, d)
        });
        Some(half_mat)
    }
}

// Gradients stuff ###################################################

/// Compute centered gradients norm at each resolution from
/// the image at the higher resolution.
///
/// As a consequence there is one less level in the gradients pyramid.
pub fn gradients_squared_norm(multires_mat: &Vec<DMatrix<u8>>) -> Vec<DMatrix<u16>> {
    let nb_levels = multires_mat.len();
    multires_mat
        .iter()
        .take(nb_levels - 1)
        .map(|mat| {
            halve(mat, gradient::bloc_squared_norm)
                .expect("There is an issue in gradients_squared_norm")
        })
        .collect()
}

/// Compute centered gradients at each resolution from
/// the image at the higher resolution.
///
/// As a consequence there is one less level in the gradients pyramid.
pub fn gradients_xy(multires_mat: &Vec<DMatrix<u8>>) -> Vec<(DMatrix<i16>, DMatrix<i16>)> {
    // TODO: maybe it would be better to return Vec<DMatrix<(i16,i16)>>,
    // to colocate the x and y gradient and do only one "halve" call?
    let nb_levels = multires_mat.len();
    multires_mat
        .iter()
        .take(nb_levels - 1)
        .map(|mat| {
            (
                halve(mat, gradient::bloc_x).expect("There is an issue in gradients_xy x."),
                halve(mat, gradient::bloc_y).expect("There is an issue in gradients_xy y."),
            )
        })
        .collect()
}
