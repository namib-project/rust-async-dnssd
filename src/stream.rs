use futures::sync::mpsc;
use futures::{self,Async};
use std::io;
use std::os::raw::c_void;
use tokio_core::reactor::{Handle,Remote};

use error::Error;
use evented::EventedDNSService;
use ffi;
use raw::DNSService;
use raw_box::RawBox;
use remote::GetRemote;

type CallbackContext<T> = mpsc::UnboundedSender<io::Result<T>>;

#[must_use = "streams do nothing unless polled"]
pub struct ServiceStream<T> {
	service: EventedDNSService,
	_sender: RawBox<CallbackContext<T>>,
	receiver: mpsc::UnboundedReceiver<io::Result<T>>,
}

impl<T> ServiceStream<T> {
	pub(crate) fn run_callback<F>(context: *mut c_void, error_code: ffi::DNSServiceErrorType, f: F)
	where
		F: FnOnce() -> io::Result<T>,
		T: ::std::fmt::Debug,
	{
		let sender = context as *mut CallbackContext<T>;
		let sender: &mut CallbackContext<T> = unsafe { &mut *sender };

		let data = Error::from(error_code).map_err(io::Error::from).and_then(|()| f());

		sender.unbounded_send(data).expect("receiver must still be alive");
	}

	pub fn new<F>(handle: &Handle, f: F) -> io::Result<Self>
	where F: FnOnce(*mut c_void) -> Result<DNSService, Error>
	{
		let (sender, receiver) = mpsc::unbounded::<io::Result<T>>();
		let sender = RawBox::new(sender);

		let service = f(sender.get_ptr() as *mut c_void)?;
		let service = EventedDNSService::new(service, handle)?;

		Ok(ServiceStream{
			service,
			_sender: sender,
			receiver,
		})
	}
}

impl<T> futures::Stream for ServiceStream<T> {
	type Item = T;
	type Error = io::Error;

	fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
		self.service.poll()?;
		match self.receiver.poll() {
			Ok(Async::Ready(None)) => Ok(Async::Ready(None)),
			Ok(Async::Ready(Some(item))) => Ok(Async::Ready(Some(item?))),
			Ok(Async::NotReady) => Ok(Async::NotReady),
			Err(()) => unreachable!(),
		}
	}
}

impl<T> GetRemote for ServiceStream<T> {
	fn remote(&self) -> &Remote {
		self.service.remote()
	}
}
