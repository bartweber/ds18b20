pub type DS18B20Result<T, E> = Result<T, DS18B20Error<E>>;

#[derive(Debug, Copy, Clone)]
pub enum DS18B20Error<E> {
    OneWireError(E),
    FamilyCodeMismatch,
    CrcMismatch,
    Timeout,
}

impl<E> From<E> for DS18B20Error<E> {
    fn from(err: E) -> Self {
        DS18B20Error::OneWireError(err)
    }
}
