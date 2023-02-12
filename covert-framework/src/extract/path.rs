use std::{ops::Deref, str::FromStr};

use covert_types::error::ApiError;

use super::{FromRequest, Request};

#[derive(Debug)]
pub struct Path<T>(pub T);

impl<T> Deref for Path<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

trait FromPathParameters: Sized {
    fn from_path_params(params: &[String]) -> Result<Self, ApiError>;
}

impl FromPathParameters for String {
    fn from_path_params(params: &[String]) -> Result<Self, ApiError> {
        params
            .get(0)
            .ok_or_else(ApiError::bad_request)
            .map(Clone::clone)
    }
}

impl<T1, T2> FromPathParameters for (T1, T2)
where
    T1: FromStr,
    T2: FromStr,
{
    fn from_path_params(params: &[String]) -> Result<Self, ApiError> {
        let param = params.get(0).ok_or_else(ApiError::bad_request)?;
        let t1 = T1::from_str(param).map_err(|_| ApiError::bad_request())?;

        let param = params.get(1).ok_or_else(ApiError::bad_request)?;
        let t2 = T2::from_str(param).map_err(|_| ApiError::bad_request())?;

        Ok((t1, t2))
    }
}

impl<T1, T2, T3> FromPathParameters for (T1, T2, T3)
where
    T1: FromStr,
    T2: FromStr,
    T3: FromStr,
{
    fn from_path_params(params: &[String]) -> Result<Self, ApiError> {
        let param = params.get(0).ok_or_else(ApiError::bad_request)?;
        let t1 = T1::from_str(param).map_err(|_| ApiError::bad_request())?;

        let param = params.get(1).ok_or_else(ApiError::bad_request)?;
        let t2 = T2::from_str(param).map_err(|_| ApiError::bad_request())?;

        let param = params.get(2).ok_or_else(ApiError::bad_request)?;
        let t3 = T3::from_str(param).map_err(|_| ApiError::bad_request())?;

        Ok((t1, t2, t3))
    }
}

impl<T> FromRequest for Path<T>
where
    T: FromPathParameters,
{
    #[tracing::instrument(level = "debug", name = "path_extractor", skip_all)]
    fn from_request(req: &mut Request) -> Result<Self, ApiError> {
        T::from_path_params(&req.params).map(Path)
    }
}
