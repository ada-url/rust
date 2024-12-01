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
            Some(out.as_str())
        }
    }

    /// Returns the stringified version of the URLSearchParams struct.
    ///
    /// ```
    /// use ada_url::URLSearchParams;
    /// let params = URLSearchParams::parse("a=1&b=2");
    /// assert_eq!(params.to_string(), "a=1&b=2");
    /// ```
    #[cfg(feature = "std")]
    #[allow(clippy::inherent_to_string)]
    pub fn to_string(&self) -> String {
        unsafe { ffi::ada_search_params_to_string(self.0).to_string() }
    }

    /// Returns all values of the key.
    ///
    /// ```
    /// use ada_url::URLSearchParams;
    /// let params = URLSearchParams::parse("a=1&a=2");
    /// let pairs = params.get_all("a");
    /// assert_eq!(pairs.get_size(), 2);
    /// ```
    pub fn get_all(&self, key: &str) -> URLSearchParamsEntry {
        unsafe {
            let strings = ffi::ada_search_params_get_all(self.0, key.as_ptr().cast(), key.len());
            let size = ffi::ada_strings_size(strings);
            URLSearchParamsEntry::new(strings, size)
        }
    }

    /// Returns all keys as an iterator
    ///
    /// ```
    /// use ada_url::URLSearchParams;
    /// let params = URLSearchParams::parse("a=1");
    /// let mut keys = params.get_keys();
    /// assert!(keys.has_next());
    pub fn get_keys(&self) -> URLSearchParamsKeysIterator {
        let iterator = unsafe { ffi::ada_search_params_get_keys(self.0) };
        URLSearchParamsKeysIterator::new(iterator)
    }

    /// Returns all keys as an iterator
    ///
    /// ```
    /// use ada_url::URLSearchParams;
    /// let params = URLSearchParams::parse("a=1");
    /// let mut values = params.get_values();
    /// assert!(values.has_next());
    pub fn get_values(&self) -> URLSearchParamsValuesIterator {
        let iterator = unsafe { ffi::ada_search_params_get_values(self.0) };
        URLSearchParamsValuesIterator::new(iterator)
    }
}

pub struct URLSearchParamsKeysIterator<'a> {
    iterator: *mut ffi::ada_url_search_params_keys_iter,
    _phantom: core::marker::PhantomData<&'a str>,
}

impl Drop for URLSearchParamsKeysIterator<'_> {
    fn drop(&mut self) {
        unsafe { ffi::ada_free_search_params_keys_iter(self.iterator) }
    }
}

impl URLSearchParamsKeysIterator<'_> {
    /// Returns true if iterator has a next value.
    pub fn has_next(&self) -> bool {
        unsafe { ffi::ada_search_params_keys_iter_has_next(self.iterator) }
    }

    /// Returns a new value if it's available
    pub fn get_next(&self) -> Option<&str> {
        if self.has_next() {
            return None;
        }
        let string = unsafe { ffi::ada_search_params_keys_iter_next(self.iterator) };
        Some(string.as_str())
    }
}

pub struct URLSearchParamsValuesIterator<'a> {
    iterator: *mut ffi::ada_url_search_params_values_iter,
    _phantom: core::marker::PhantomData<&'a str>,
}

impl<'a> URLSearchParamsKeysIterator<'a> {
    fn new(iterator: *mut ffi::ada_url_search_params_keys_iter) -> URLSearchParamsKeysIterator<'a> {
        URLSearchParamsKeysIterator {
            iterator,
            _phantom: core::marker::PhantomData,
        }
    }
}

impl Drop for URLSearchParamsValuesIterator<'_> {
    fn drop(&mut self) {
        unsafe { ffi::ada_free_search_params_values_iter(self.iterator) }
    }
}

impl<'a> URLSearchParamsValuesIterator<'a> {
    fn new(
        iterator: *mut ffi::ada_url_search_params_values_iter,
    ) -> URLSearchParamsValuesIterator<'a> {
        URLSearchParamsValuesIterator {
            iterator,
            _phantom: core::marker::PhantomData,
        }
    }
}

impl URLSearchParamsValuesIterator<'_> {
    /// Returns true if iterator has a next value.
    pub fn has_next(&self) -> bool {
        unsafe { ffi::ada_search_params_values_iter_has_next(self.iterator) }
    }

    /// Returns a new value if it's available
    pub fn get_next(&self) -> Option<&str> {
        if self.has_next() {
            return None;
        }
        let string = unsafe { ffi::ada_search_params_values_iter_next(self.iterator) };
        Some(string.as_str())
    }
}

pub struct URLSearchParamsEntry<'a> {
    strings: *mut ffi::ada_strings,
    size: usize,
    _phantom: core::marker::PhantomData<&'a str>,
}

impl<'a> URLSearchParamsEntry<'a> {
    fn new(strings: *mut ffi::ada_strings, size: usize) -> URLSearchParamsEntry<'a> {
        URLSearchParamsEntry {
            strings,
            size,
            _phantom: core::marker::PhantomData,
        }
    }

    /// Returns whether the key value pair is empty or not
    ///
    /// ```
    /// use ada_url::URLSearchParams;
    /// let params = URLSearchParams::parse("a=1&b=2");
    /// let pairs = params.get_all("a");
    /// assert_eq!(pairs.is_empty(), false);
    /// ```
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Returns the size of the key value pairs
    ///
    /// ```
    /// use ada_url::URLSearchParams;
    /// let params = URLSearchParams::parse("a=1&b=2");
    /// let pairs = params.get_all("a");
    /// assert_eq!(pairs.get_size(), 1);
    /// ```
    pub fn get_size(&self) -> usize {
        self.size
    }

    /// Get an entry by index
    ///
    /// ```
    /// use ada_url::URLSearchParams;
    /// let params = URLSearchParams::parse("a=1&a=2");
    /// let pairs = params.get_all("a");
    /// assert_eq!(pairs.get_size(), 2);
    /// assert_eq!(pairs.get(0), Some("1"));
    /// assert_eq!(pairs.get(1), Some("2"));
    /// assert_eq!(pairs.get(2), None);
    /// assert_eq!(pairs.get(55), None);
    /// ```
    pub fn get(&self, index: usize) -> Option<&str> {
        if self.size == 0 || index > self.size - 1 {
            return None;
        }

        unsafe {
            let string = ffi::ada_strings_get(self.strings, index);
            Some(string.as_str())
        }
    }
}

impl Drop for URLSearchParamsEntry<'_> {
    fn drop(&mut self) {
        unsafe { ffi::ada_free_strings(self.strings) }
    }
}

#[cfg(feature = "std")]
impl<'a> From<URLSearchParamsEntry<'a>> for Vec<&'a str> {
    fn from(val: URLSearchParamsEntry<'a>) -> Self {
        let mut vec = Vec::with_capacity(val.size);
        unsafe {
            for index in 0..val.size {
                let string = ffi::ada_strings_get(val.strings, index);
                let slice = core::slice::from_raw_parts(string.data.cast(), string.length);
                vec.push(core::str::from_utf8_unchecked(slice));
            }
        }
        vec
    }
}
