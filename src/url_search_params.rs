use crate::ffi;

pub struct URLSearchParams(*mut ffi::ada_url_search_params);

impl Drop for URLSearchParams {
    fn drop(&mut self) {
        unsafe { ffi::ada_free_search_params(self.0) }
    }
}

impl URLSearchParams {
    /// Parses an return a URLSearchParams struct.
    ///
    /// ```
    /// use ada_url::URLSearchParams;
    /// let params = URLSearchParams::parse("a=1&b=2");
    /// assert_eq!(params.get("a"), Some("1"));
    /// assert_eq!(params.get("b"), Some("2"));
    /// ```
    pub fn parse(input: &str) -> Self {
        Self(unsafe { ffi::ada_parse_search_params(input.as_ptr().cast(), input.len()) })
    }

    /// Returns the size of the URLSearchParams struct.
    ///
    /// ```
    /// use ada_url::URLSearchParams;
    /// let params = URLSearchParams::parse("a=1&b=2");
    /// assert_eq!(params.size(), 2);
    /// ```
    pub fn size(&self) -> usize {
        unsafe { ffi::ada_search_params_size(self.0) }
    }

    /// Sorts the keys of the URLSearchParams struct.
    pub fn sort(&self) {
        unsafe { ffi::ada_search_params_sort(self.0) }
    }

    /// Appends a key/value to the URLSearchParams struct.
    pub fn append(&self, key: &str, value: &str) {
        unsafe {
            ffi::ada_search_params_append(
                self.0,
                key.as_ptr().cast(),
                key.len(),
                value.as_ptr().cast(),
                value.len(),
            )
        }
    }

    /// Removes all keys pre-existing keys from the URLSearchParams struct
    /// and appends the new key/value.
    ///
    /// ```
    /// use ada_url::URLSearchParams;
    /// let params = URLSearchParams::parse("a=1&b=2");
    /// params.set("a", "3");
    /// assert_eq!(params.get("a"), Some("3"));
    /// ```
    pub fn set(&self, key: &str, value: &str) {
        unsafe {
            ffi::ada_search_params_set(
                self.0,
                key.as_ptr().cast(),
                key.len(),
                value.as_ptr().cast(),
                value.len(),
            )
        }
    }

    /// Removes a key/value from the URLSearchParams struct.
    /// Depending on the value parameter, it will either remove
    /// the key/value pair or just the key.
    ///
    /// ```
    /// use ada_url::URLSearchParams;
    /// let params = URLSearchParams::parse("a=1&b=2");
    /// params.remove("a", Some("1"));
    /// assert_eq!(params.get("a"), None);
    /// ```
    pub fn remove(&self, key: &str, value: Option<&str>) {
        if let Some(value) = value {
            unsafe {
                ffi::ada_search_params_remove_value(
                    self.0,
                    key.as_ptr().cast(),
                    key.len(),
                    value.as_ptr().cast(),
                    value.len(),
                )
            }
        } else {
            unsafe { ffi::ada_search_params_remove(self.0, key.as_ptr().cast(), key.len()) }
        }
    }

    /// Retruns true if the URLSearchParams struct contains the key.
    ///
    /// ```
    /// use ada_url::URLSearchParams;
    /// let params = URLSearchParams::parse("a=1&b=2");
    /// assert_eq!(params.has("a", None), true);
    /// ```
    pub fn has(&self, key: &str, value: Option<&str>) -> bool {
        if let Some(value) = value {
            unsafe {
                ffi::ada_search_params_has_value(
                    self.0,
                    key.as_ptr().cast(),
                    key.len(),
                    value.as_ptr().cast(),
                    value.len(),
                )
            }
        } else {
            unsafe { ffi::ada_search_params_has(self.0, key.as_ptr().cast(), key.len()) }
        }
    }

    /// Returns the value of the key.
    ///
    /// ```
    /// use ada_url::URLSearchParams;
    /// let params = URLSearchParams::parse("a=1&b=2");
    /// assert_eq!(params.get("a"), Some("1"));
    /// assert_eq!(params.get("c"), None);
    /// ```
    pub fn get(&self, key: &str) -> Option<&str> {
        unsafe {
            let out = ffi::ada_search_params_get(self.0, key.as_ptr().cast(), key.len());

            if out.data.is_null() {
                return None;
            }
            let slice = core::slice::from_raw_parts(out.data.cast(), out.length);
            Some(core::str::from_utf8_unchecked(slice))
        }
    }

    /// Returns the stringified version of the URLSearchParams struct.
    ///
    /// ```
    /// use ada_url::URLSearchParams;
    /// let params = URLSearchParams::parse("a=1&b=2");
    /// assert_eq!(params.to_string(), "a=1&b=2");
    /// ```
    pub fn to_string(&self) -> &str {
        unsafe {
            let out = ffi::ada_search_params_to_string(self.0);
            let slice = core::slice::from_raw_parts(out.data.cast(), out.length);
            core::str::from_utf8_unchecked(slice)
        }
    }

    /// Returns all values of the key.
    ///
    /// ```
    /// use ada_url::URLSearchParams;
    /// let params = URLSearchParams::parse("a=1&a=2");
    /// assert_eq!(params.get_all("a"), vec!["1", "2"]);
    /// ```
    pub fn get_all(&self, key: &str) -> Vec<&str> {
        unsafe {
            let strings = ffi::ada_search_params_get_all(self.0, key.as_ptr().cast(), key.len());
            let size = ffi::ada_strings_size(strings);
            let mut out = Vec::with_capacity(size);

            if size == 0 {
                return out;
            }

            for index in 0..size {
                let string = ffi::ada_strings_get(strings, index);
                let slice = core::slice::from_raw_parts(string.data.cast(), string.length);
                out.push(core::str::from_utf8_unchecked(slice));
            }

            out
        }
    }
}
