use crate::{ffi, ParseUrlError};

pub struct UrlSearchParams(*mut ffi::ada_url_search_params);

impl Drop for UrlSearchParams {
    fn drop(&mut self) {
        unsafe { ffi::ada_free_search_params(self.0) }
    }
}

impl UrlSearchParams {
    /// Parses an return a UrlSearchParams struct.
    ///
    /// ```
    /// use ada_url::UrlSearchParams;
    /// let params = UrlSearchParams::parse("a=1&b=2")
    ///     .expect("This is a valid UrlSearchParams. Should have parsed it.");
    /// assert_eq!(params.get("a"), Some("1"));
    /// assert_eq!(params.get("b"), Some("2"));
    /// ```
    pub fn parse<Input>(input: Input) -> Result<Self, ParseUrlError<Input>>
    where
        Input: AsRef<str>,
    {
        Ok(Self(unsafe {
            ffi::ada_parse_search_params(input.as_ref().as_ptr().cast(), input.as_ref().len())
        }))
    }

    /// Returns the size of the UrlSearchParams struct.
    ///
    /// ```
    /// use ada_url::UrlSearchParams;
    /// let params = UrlSearchParams::parse("a=1&b=2")
    ///     .expect("This is a valid UrlSearchParams. Should have parsed it.");
    /// assert_eq!(params.len(), 2);
    /// ```
    pub fn len(&self) -> usize {
        unsafe { ffi::ada_search_params_size(self.0) }
    }

    /// Returns true if no entries exist in the UrlSearchParams.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Sorts the keys of the UrlSearchParams struct.
    pub fn sort(&mut self) {
        unsafe { ffi::ada_search_params_sort(self.0) }
    }

    /// Appends a key/value to the UrlSearchParams struct.
    pub fn append(&mut self, key: &str, value: &str) {
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

    /// Removes all pre-existing keys from the UrlSearchParams struct
    /// and appends the new key/value.
    ///
    /// ```
    /// use ada_url::UrlSearchParams;
    /// let mut params = UrlSearchParams::parse("a=1&b=2")
    ///     .expect("This is a valid UrlSearchParams. Should have parsed it.");
    /// params.set("a", "3");
    /// assert_eq!(params.get("a"), Some("3"));
    /// ```
    pub fn set(&mut self, key: &str, value: &str) {
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

    /// Removes a key/value from the UrlSearchParams struct.
    /// Depending on the value parameter, it will either remove
    /// the key/value pair or just the key.
    ///
    /// ```
    /// use ada_url::UrlSearchParams;
    /// let mut params = UrlSearchParams::parse("a=1&b=2")
    ///     .expect("This is a valid UrlSearchParams. Should have parsed it.");
    /// params.remove("a", Some("1"));
    /// assert_eq!(params.get("a"), None);
    /// ```
    pub fn remove(&mut self, key: &str, value: Option<&str>) {
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

    /// Returns whether the [`UrlSearchParams`] contains the `key`.
    ///
    /// ```
    /// use ada_url::UrlSearchParams;
    /// let params = UrlSearchParams::parse("a=1&b=2")
    ///     .expect("This is a valid UrlSearchParams. Should have parsed it.");
    /// assert_eq!(params.contains("a", None), true);
    /// ```
    pub fn contains(&self, key: &str, value: Option<&str>) -> bool {
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
    /// use ada_url::UrlSearchParams;
    /// let params = UrlSearchParams::parse("a=1&b=2")
    ///     .expect("This is a valid UrlSearchParams. Should have parsed it.");
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

    /// Returns all values of the key.
    ///
    /// ```
    /// use ada_url::UrlSearchParams;
    /// let params = UrlSearchParams::parse("a=1&a=2")
    ///     .expect("This is a valid UrlSearchParams. Should have parsed it.");
    /// let pairs = params.get_all("a");
    /// assert_eq!(pairs.len(), 2);
    /// ```
    pub fn get_all(&self, key: &str) -> UrlSearchParamsEntry {
        unsafe {
            let strings = ffi::ada_search_params_get_all(self.0, key.as_ptr().cast(), key.len());
            let size = ffi::ada_strings_size(strings);
            UrlSearchParamsEntry::new(strings, size)
        }
    }

    /// Returns all keys as an iterator
    ///
    /// ```
    /// use ada_url::UrlSearchParams;
    /// let params = UrlSearchParams::parse("a=1")
    ///     .expect("This is a valid UrlSearchParams. Should have parsed it.");
    /// let mut keys = params.keys();
    /// assert!(keys.next().is_some());
    pub fn keys(&self) -> UrlSearchParamsKeyIterator {
        let iterator = unsafe { ffi::ada_search_params_get_keys(self.0) };
        UrlSearchParamsKeyIterator::new(iterator)
    }

    /// Returns all keys as an iterator
    ///
    /// ```
    /// use ada_url::UrlSearchParams;
    /// let params = UrlSearchParams::parse("a=1")
    ///     .expect("This is a valid UrlSearchParams. Should have parsed it.");
    /// let mut values = params.values();
    /// assert!(values.next().is_some());
    pub fn values(&self) -> UrlSearchParamsValueIterator {
        let iterator = unsafe { ffi::ada_search_params_get_values(self.0) };
        UrlSearchParamsValueIterator::new(iterator)
    }

    /// Returns all entries as an iterator
    ///
    /// ```
    /// use ada_url::UrlSearchParams;
    /// let params = UrlSearchParams::parse("a=1")
    ///     .expect("This is a valid UrlSearchParams. Should have parsed it.");
    /// let mut entries = params.entries();
    /// assert_eq!(entries.next(), Some(("a", "1")));
    /// ```
    pub fn entries(&self) -> UrlSearchParamsEntryIterator {
        let iterator = unsafe { ffi::ada_search_params_get_entries(self.0) };
        UrlSearchParamsEntryIterator::new(iterator)
    }
}

#[cfg(feature = "std")]
impl core::str::FromStr for UrlSearchParams {
    type Err = ParseUrlError<Box<str>>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).map_err(|ParseUrlError { input }| ParseUrlError {
            input: input.into(),
        })
    }
}

/// Returns the stringified version of the UrlSearchParams struct.
///
/// ```
/// use ada_url::UrlSearchParams;
/// let params = UrlSearchParams::parse("a=1&b=2")
///     .expect("This is a valid UrlSearchParams. Should have parsed it.");
/// assert_eq!(params.to_string(), "a=1&b=2");
/// ```
impl core::fmt::Display for UrlSearchParams {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(unsafe { ffi::ada_search_params_to_string(self.0).as_ref() })
    }
}

#[cfg(feature = "std")]
impl<Input> Extend<(Input, Input)> for UrlSearchParams
where
    Input: AsRef<str>,
{
    /// Supports extending UrlSearchParams through an iterator.
    ///
    ///```
    /// use ada_url::UrlSearchParams;
    /// let mut params = UrlSearchParams::parse("a=1&b=2")
    ///     .expect("This is a valid UrlSearchParams. Should have parsed it.");
    /// assert_eq!(params.len(), 2);
    /// params.extend([("foo", "bar")]);
    /// assert_eq!(params.len(), 3);
    /// ```
    fn extend<T: IntoIterator<Item = (Input, Input)>>(&mut self, iter: T) {
        for item in iter {
            self.append(item.0.as_ref(), item.1.as_ref());
        }
    }
}

#[cfg(feature = "std")]
impl<Input> FromIterator<(Input, Input)> for UrlSearchParams
where
    Input: AsRef<str>,
{
    /// Converts an iterator to UrlSearchParams
    ///
    /// ```
    /// use ada_url::UrlSearchParams;
    /// let iterator = std::iter::repeat(("hello", "world")).take(5);
    /// let params = UrlSearchParams::from_iter(iterator);
    /// assert_eq!(params.len(), 5);
    /// ```
    fn from_iter<T: IntoIterator<Item = (Input, Input)>>(iter: T) -> Self {
        let mut params = UrlSearchParams::parse("")
            .expect("Failed to parse empty string. This is likely due to a bug");
        for item in iter {
            params.append(item.0.as_ref(), item.1.as_ref());
        }
        params
    }
}

pub struct UrlSearchParamsKeyIterator<'a> {
    iterator: *mut ffi::ada_url_search_params_keys_iter,
    _phantom: core::marker::PhantomData<&'a str>,
}

impl Drop for UrlSearchParamsKeyIterator<'_> {
    fn drop(&mut self) {
        unsafe { ffi::ada_free_search_params_keys_iter(self.iterator) }
    }
}

impl<'a> Iterator for UrlSearchParamsKeyIterator<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let has_next = unsafe { ffi::ada_search_params_keys_iter_has_next(self.iterator) };
        if has_next {
            let string = unsafe { ffi::ada_search_params_keys_iter_next(self.iterator) };
            Some(string.as_str())
        } else {
            None
        }
    }
}

pub struct UrlSearchParamsValueIterator<'a> {
    iterator: *mut ffi::ada_url_search_params_values_iter,
    _phantom: core::marker::PhantomData<&'a str>,
}

impl<'a> UrlSearchParamsKeyIterator<'a> {
    fn new(iterator: *mut ffi::ada_url_search_params_keys_iter) -> UrlSearchParamsKeyIterator<'a> {
        UrlSearchParamsKeyIterator {
            iterator,
            _phantom: core::marker::PhantomData,
        }
    }
}

impl Drop for UrlSearchParamsValueIterator<'_> {
    fn drop(&mut self) {
        unsafe { ffi::ada_free_search_params_values_iter(self.iterator) }
    }
}

impl<'a> Iterator for UrlSearchParamsValueIterator<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let has_next = unsafe { ffi::ada_search_params_values_iter_has_next(self.iterator) };
        if has_next {
            let string = unsafe { ffi::ada_search_params_values_iter_next(self.iterator) };
            Some(string.as_str())
        } else {
            None
        }
    }
}

impl<'a> UrlSearchParamsValueIterator<'a> {
    fn new(
        iterator: *mut ffi::ada_url_search_params_values_iter,
    ) -> UrlSearchParamsValueIterator<'a> {
        UrlSearchParamsValueIterator {
            iterator,
            _phantom: core::marker::PhantomData,
        }
    }
}

pub struct UrlSearchParamsEntry<'a> {
    strings: *mut ffi::ada_strings,
    size: usize,
    _phantom: core::marker::PhantomData<&'a str>,
}

impl<'a> UrlSearchParamsEntry<'a> {
    fn new(strings: *mut ffi::ada_strings, size: usize) -> UrlSearchParamsEntry<'a> {
        UrlSearchParamsEntry {
            strings,
            size,
            _phantom: core::marker::PhantomData,
        }
    }

    /// Returns whether the key value pair is empty or not
    ///
    /// ```
    /// use ada_url::UrlSearchParams;
    /// let params = UrlSearchParams::parse("a=1&b=2")
    ///     .expect("This is a valid UrlSearchParams. Should have parsed it.");
    /// let pairs = params.get_all("a");
    /// assert_eq!(pairs.is_empty(), false);
    /// ```
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Returns the size of the key value pairs
    ///
    /// ```
    /// use ada_url::UrlSearchParams;
    /// let params = UrlSearchParams::parse("a=1&b=2")
    ///     .expect("This is a valid UrlSearchParams. Should have parsed it.");
    /// let pairs = params.get_all("a");
    /// assert_eq!(pairs.len(), 1);
    /// ```
    pub fn len(&self) -> usize {
        self.size
    }

    /// Get an entry by index
    ///
    /// ```
    /// use ada_url::UrlSearchParams;
    /// let params = UrlSearchParams::parse("a=1&a=2")
    ///     .expect("This is a valid UrlSearchParams. Should have parsed it.");
    /// let pairs = params.get_all("a");
    /// assert_eq!(pairs.len(), 2);
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

impl Drop for UrlSearchParamsEntry<'_> {
    fn drop(&mut self) {
        unsafe { ffi::ada_free_strings(self.strings) }
    }
}

#[cfg(feature = "std")]
impl<'a> From<UrlSearchParamsEntry<'a>> for Vec<&'a str> {
    fn from(val: UrlSearchParamsEntry<'a>) -> Self {
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

pub struct UrlSearchParamsEntryIterator<'a> {
    iterator: *mut ffi::ada_url_search_params_entries_iter,
    _phantom: core::marker::PhantomData<&'a str>,
}

impl<'a> UrlSearchParamsEntryIterator<'a> {
    fn new(
        iterator: *mut ffi::ada_url_search_params_entries_iter,
    ) -> UrlSearchParamsEntryIterator<'a> {
        UrlSearchParamsEntryIterator {
            iterator,
            _phantom: core::marker::PhantomData,
        }
    }
}

impl Drop for UrlSearchParamsEntryIterator<'_> {
    fn drop(&mut self) {
        unsafe { ffi::ada_free_search_params_entries_iter(self.iterator) }
    }
}

impl<'a> Iterator for UrlSearchParamsEntryIterator<'a> {
    type Item = (&'a str, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        let has_next = unsafe { ffi::ada_search_params_entries_iter_has_next(self.iterator) };
        if has_next {
            let pair = unsafe { ffi::ada_search_params_entries_iter_next(self.iterator) };
            Some((pair.key.as_str(), pair.value.as_str()))
        } else {
            None
        }
    }
}
