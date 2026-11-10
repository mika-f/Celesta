//! The MFT driver: one Media Foundation transform, synchronous or
//! asynchronous, fed input samples and drained of output samples on the
//! serving thread.
//!
//! - [`candidates`] lists the transforms `MFTEnumEx` gives for a category
//!   and a pair of subtypes, hardware first with the hardware flags.
//! - [`first_accepting`] tries them in order, releasing each that refuses.
//! - A synchronous transform is fed with `ProcessInput` and read with
//!   `ProcessOutput` until it needs more input.
//! - An asynchronous one (`MF_TRANSFORM_ASYNC`, which Task 1 found the
//!   enumeration flags do not tell) is unlocked, fed on
//!   `METransformNeedInput`, and read on `METransformHaveOutput`. Its events
//!   are polled every millisecond with `GetEvent(MF_EVENT_FLAG_NO_WAIT)`
//!   ([`wait_for`]); a transform silent for [`STALL`] has stopped
//!   answering.

use crate::mf::media_type::set_color;
use celesta_media::MediaError;
use std::mem::ManuallyDrop;
use std::thread;
use std::time::{Duration, Instant};
use windows::Win32::Media::MediaFoundation::{
    ICodecAPI, IMFActivate, IMFAttributes, IMFMediaEventGenerator, IMFMediaType, IMFSample,
    IMFShutdown, IMFTransform, MEError, METransformDrainComplete, METransformHaveOutput,
    METransformNeedInput, MF_E_NO_EVENTS_AVAILABLE, MF_E_NOTACCEPTING,
    MF_E_TRANSFORM_NEED_MORE_INPUT, MF_E_TRANSFORM_STREAM_CHANGE, MF_EVENT_FLAG_NO_WAIT,
    MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_TRANSFORM_ASYNC, MF_TRANSFORM_ASYNC_UNLOCK,
    MF_TRANSFORM_FLAGS_Attribute, MFCreateMemoryBuffer, MFCreateSample, MFMediaType_Audio,
    MFMediaType_Video, MFT_CATEGORY_AUDIO_ENCODER, MFT_ENUM_FLAG, MFT_ENUM_FLAG_ASYNCMFT,
    MFT_ENUM_FLAG_HARDWARE, MFT_ENUM_FLAG_LOCALMFT, MFT_ENUM_FLAG_SORTANDFILTER,
    MFT_ENUM_FLAG_SYNCMFT, MFT_ENUM_HARDWARE_URL_Attribute, MFT_FRIENDLY_NAME_Attribute,
    MFT_MESSAGE_COMMAND_DRAIN, MFT_MESSAGE_NOTIFY_BEGIN_STREAMING,
    MFT_MESSAGE_NOTIFY_END_OF_STREAM, MFT_MESSAGE_NOTIFY_START_OF_STREAM, MFT_OUTPUT_DATA_BUFFER,
    MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES, MFT_OUTPUT_STREAM_PROVIDES_SAMPLES,
    MFT_REGISTER_TYPE_INFO, MFTEnumEx,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows_core::{GUID, Interface, PWSTR};

/// How long a transform may give no event before it has stopped answering
/// (P5's stall rule).
pub(crate) const STALL: Duration = Duration::from_secs(60);

/// How often an asynchronous transform's events are polled.
const POLL: Duration = Duration::from_millis(1);

/// How many stream changes in a row one `ProcessOutput` may answer before
/// the transform is taken to be changing its output type for ever.
const STREAM_CHANGES: u32 = 8;

/// The flags that list hardware and software transforms, best first.
pub(crate) const HARDWARE_FLAGS: MFT_ENUM_FLAG = MFT_ENUM_FLAG(
    MFT_ENUM_FLAG_HARDWARE.0
        | MFT_ENUM_FLAG_SYNCMFT.0
        | MFT_ENUM_FLAG_ASYNCMFT.0
        | MFT_ENUM_FLAG_LOCALMFT.0
        | MFT_ENUM_FLAG_SORTANDFILTER.0,
);

/// The flags that list only synchronous software transforms.
pub(crate) const SOFTWARE_FLAGS: MFT_ENUM_FLAG =
    MFT_ENUM_FLAG(MFT_ENUM_FLAG_SYNCMFT.0 | MFT_ENUM_FLAG_SORTANDFILTER.0);

/// The error for a Media Foundation call `call` that failed with `error`.
pub(crate) fn failed(call: &str, error: &windows_core::Error) -> MediaError {
    MediaError::Codec(format!("{call} failed: 0x{:08X}", error.code().0 as u32))
}

/// `result`, its error as [`failed`] for `call`.
pub(crate) fn check<T>(call: &str, result: windows_core::Result<T>) -> Result<T, MediaError> {
    result.map_err(|error| failed(call, &error))
}

/// The transforms of `category` that `MFTEnumEx` lists from `input` to
/// `output` (subtypes of the category's major type), in its order: with
/// [`HARDWARE_FLAGS`] when `hardware`, else [`SOFTWARE_FLAGS`]. Media
/// Foundation must have started.
pub(crate) fn candidates(
    category: GUID,
    input: GUID,
    output: GUID,
    hardware: bool,
) -> Result<Vec<IMFActivate>, MediaError> {
    let major = if category == MFT_CATEGORY_AUDIO_ENCODER {
        MFMediaType_Audio
    } else {
        MFMediaType_Video
    };
    let input = MFT_REGISTER_TYPE_INFO {
        guidMajorType: major,
        guidSubtype: input,
    };
    let output = MFT_REGISTER_TYPE_INFO {
        guidMajorType: major,
        guidSubtype: output,
    };
    let flags = if hardware {
        HARDWARE_FLAGS
    } else {
        SOFTWARE_FLAGS
    };
    let mut activates: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count = 0u32;
    // SAFETY: the type infos outlive the call, and `activates` and `count`
    // are its out-parameters.
    check("MFTEnumEx", unsafe {
        MFTEnumEx(
            category,
            flags,
            Some(&input),
            Some(&output),
            &mut activates,
            &mut count,
        )
    })?;
    if activates.is_null() {
        return Ok(Vec::new());
    }
    // SAFETY: on success `activates` points to `count` interface pointers in
    // memory from `CoTaskMemAlloc`; each is taken (its reference moves into
    // the list), and the array is freed once.
    unsafe {
        let listed = std::slice::from_raw_parts_mut(activates, count as usize)
            .iter_mut()
            .filter_map(Option::take)
            .collect();
        CoTaskMemFree(Some(activates.cast_const().cast()));
        Ok(listed)
    }
}

/// The string attribute `key` of `attributes`, if it has one.
pub(crate) fn string_attribute(attributes: &IMFAttributes, key: &GUID) -> Option<String> {
    let mut value = PWSTR::null();
    let mut length = 0;
    // SAFETY: `value` receives a string from `CoTaskMemAlloc`, freed once
    // here after it is copied.
    unsafe {
        attributes
            .GetAllocatedString(key, &mut value, &mut length)
            .ok()?;
        let text = value.to_string().ok();
        CoTaskMemFree(Some(value.0.cast_const().cast()));
        text
    }
}

/// The blob attribute `key` of `attributes`, if it has one.
pub(crate) fn blob_attribute(attributes: &IMFAttributes, key: &GUID) -> Option<Vec<u8>> {
    let mut pointer = std::ptr::null_mut();
    let mut size = 0;
    // SAFETY: `pointer` receives `size` bytes from `CoTaskMemAlloc`, freed
    // once here after they are copied.
    unsafe {
        attributes
            .GetAllocatedBlob(key, &mut pointer, &mut size)
            .ok()?;
        let bytes = if pointer.is_null() {
            Vec::new()
        } else {
            std::slice::from_raw_parts(pointer, size as usize).to_vec()
        };
        CoTaskMemFree(Some(pointer.cast_const().cast()));
        Some(bytes)
    }
}

/// A sample of one memory buffer holding `bytes`, at `time` for `duration`
/// (hns).
pub(crate) fn make_sample(bytes: &[u8], time: i64, duration: i64) -> Result<IMFSample, MediaError> {
    let length = u32::try_from(bytes.len()).map_err(|_| {
        MediaError::Codec(format!("a sample of {} bytes is too large", bytes.len()))
    })?;
    // SAFETY: the buffer is locked while `bytes` is copied into its
    // `length` bytes, then unlocked; the sample holds a reference to it.
    unsafe {
        let buffer = check("MFCreateMemoryBuffer", MFCreateMemoryBuffer(length.max(1)))?;
        let mut pointer = std::ptr::null_mut();
        check(
            "IMFMediaBuffer::Lock",
            buffer.Lock(&mut pointer, None, None),
        )?;
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer, bytes.len());
        check("IMFMediaBuffer::Unlock", buffer.Unlock())?;
        check(
            "IMFMediaBuffer::SetCurrentLength",
            buffer.SetCurrentLength(length),
        )?;
        let sample = check("MFCreateSample", MFCreateSample())?;
        check("IMFSample::AddBuffer", sample.AddBuffer(&buffer))?;
        check("IMFSample::SetSampleTime", sample.SetSampleTime(time))?;
        check(
            "IMFSample::SetSampleDuration",
            sample.SetSampleDuration(duration),
        )?;
        Ok(sample)
    }
}

/// The bytes of `sample`'s buffers, back to back.
pub(crate) fn sample_bytes(sample: &IMFSample) -> Result<Vec<u8>, MediaError> {
    // SAFETY: the contiguous buffer is locked while its `length` bytes are
    // copied out, then unlocked.
    unsafe {
        let buffer = check(
            "IMFSample::ConvertToContiguousBuffer",
            sample.ConvertToContiguousBuffer(),
        )?;
        let mut pointer = std::ptr::null_mut();
        let mut length = 0u32;
        check(
            "IMFMediaBuffer::Lock",
            buffer.Lock(&mut pointer, None, Some(&mut length)),
        )?;
        let bytes = if pointer.is_null() {
            Vec::new()
        } else {
            std::slice::from_raw_parts(pointer, length as usize).to_vec()
        };
        check("IMFMediaBuffer::Unlock", buffer.Unlock())?;
        Ok(bytes)
    }
}

/// Polls `poll` every millisecond until it answers: what it answers, its
/// error, or, after `stall` with no answer, `{name} stopped answering`.
pub(crate) fn wait_for<T>(
    mut poll: impl FnMut() -> Result<Option<T>, MediaError>,
    stall: Duration,
    name: &str,
) -> Result<T, MediaError> {
    let start = Instant::now();
    loop {
        if let Some(answer) = poll()? {
            return Ok(answer);
        }
        if start.elapsed() >= stall {
            return Err(MediaError::Codec(format!("{name} stopped answering")));
        }
        thread::sleep(POLL);
    }
}

/// The first of `candidates` that `try_one` accepts, tried in order: what
/// it gave, or, when none accepts, the most specific refusal. That is the
/// last `MediaError::InvalidSettings` (a candidate that refused a setting
/// the user can change, such as its rate control: deviation 78); else
/// `component` is unavailable, with every refusal in the reason (Task 1:
/// `MFTEnumEx` lists the DX12 encoders, which refuse every output type, so
/// a listed encoder may still open none). Whatever `try_one` made of a
/// refused candidate is dropped before the next is tried.
pub(crate) fn first_accepting<C, S>(
    component: &str,
    candidates: &[C],
    mut try_one: impl FnMut(&C) -> Result<S, MediaError>,
) -> Result<S, MediaError> {
    let mut settings = None;
    let mut refusals = Vec::new();
    for candidate in candidates {
        match try_one(candidate) {
            Ok(session) => return Ok(session),
            Err(error @ MediaError::InvalidSettings { .. }) => settings = Some(error),
            Err(error) => refusals.push(error.to_string()),
        }
    }
    if let Some(error) = settings {
        return Err(error);
    }
    Err(MediaError::ComponentUnavailable {
        component: component.to_owned(),
        reason: if refusals.is_empty() {
            "Media Foundation listed no transform to try".to_owned()
        } else {
            format!(
                "every transform Media Foundation lists refused: {}",
                refusals.join("; ")
            )
        },
    })
}

/// `activate`'s friendly name (`MFT_FRIENDLY_NAME_Attribute`).
pub(crate) fn friendly_name(activate: &IMFActivate) -> String {
    string_attribute(activate, &MFT_FRIENDLY_NAME_Attribute)
        .unwrap_or_else(|| "a Media Foundation transform".to_owned())
}

/// Whether `activate` is a hardware transform: its
/// `MF_TRANSFORM_FLAGS_Attribute` has `MFT_ENUM_FLAG_HARDWARE` (Task 1:
/// NVIDIA's are `0x4`, the DX12 encoders' `0x6`), or it has a hardware URL.
pub(crate) fn is_hardware(activate: &IMFActivate) -> bool {
    // SAFETY: a plain COM call on a live interface.
    let flags = unsafe { activate.GetUINT32(&MF_TRANSFORM_FLAGS_Attribute) }.unwrap_or(0);
    flags & MFT_ENUM_FLAG_HARDWARE.0 as u32 != 0
        || string_attribute(activate, &MFT_ENUM_HARDWARE_URL_Attribute).is_some()
}

/// `error` from the transform `name`, its message led by the name, so the
/// refusal [`first_accepting`] keeps says which transform refused. A
/// settings refusal, which names the transform itself, is left as it is.
pub(crate) fn named(name: &str, error: MediaError) -> MediaError {
    match error {
        MediaError::Codec(message) if !message.starts_with(name) => {
            MediaError::Codec(format!("{name}: {message}"))
        }
        other => other,
    }
}

/// Counts one more stream change in a row of the transform `name` into
/// `changes`: an error once there are more than [`STREAM_CHANGES`].
fn count_stream_change(changes: &mut u32, name: &str) -> Result<(), MediaError> {
    *changes += 1;
    if *changes > STREAM_CHANGES {
        return Err(MediaError::Codec(format!(
            "{name} kept changing its output type"
        )));
    }
    Ok(())
}

/// What an asynchronous transform's event asks.
enum Event {
    NeedInput,
    HaveOutput,
    DrainComplete,
    /// Any other event, which the driver does not act on.
    Other,
}

/// One activated transform, synchronous or asynchronous.
pub(crate) struct Transform {
    transform: IMFTransform,
    /// What it was activated from, shut down on drop.
    activate: IMFActivate,
    /// Its event generator, when it is asynchronous.
    events: Option<IMFMediaEventGenerator>,
    /// Its friendly name, for errors and tests.
    name: String,
    /// Whether it allocates its output samples.
    provides: bool,
    /// The size of the output samples the driver allocates otherwise.
    output_size: u32,
    /// `METransformNeedInput` events not yet answered with an input.
    need_input: u32,
    /// Whether the types are set and it has not been drained.
    streaming: bool,
    /// Whether it is a hardware transform.
    hardware: bool,
    /// The output type's major type and subtype, once set: a stream change
    /// must keep the subtype.
    output_kind: Option<(GUID, GUID)>,
}

impl Transform {
    /// Activates `activate`'s transform, unlocking it when it is
    /// asynchronous.
    pub(crate) fn activate(activate: &IMFActivate) -> Result<Self, MediaError> {
        let name = friendly_name(activate);
        let hardware = is_hardware(activate);
        // SAFETY: plain COM calls on live interfaces.
        let transform: IMFTransform = check("IMFActivate::ActivateObject", unsafe {
            activate.ActivateObject::<IMFTransform>()
        })?;
        let mut opened = Self {
            transform,
            activate: activate.clone(),
            events: None,
            name,
            provides: false,
            output_size: 0,
            need_input: 0,
            streaming: false,
            hardware,
            output_kind: None,
        };
        // SAFETY: plain COM calls on live interfaces.
        let attributes = unsafe { opened.transform.GetAttributes() }.ok();
        let asynchronous = attributes
            .as_ref()
            // SAFETY: as above.
            .and_then(|attributes| unsafe { attributes.GetUINT32(&MF_TRANSFORM_ASYNC) }.ok())
            .is_some_and(|value| value != 0);
        if let (true, Some(attributes)) = (asynchronous, &attributes) {
            // SAFETY: as above.
            check("SetUINT32(MF_TRANSFORM_ASYNC_UNLOCK)", unsafe {
                attributes.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1)
            })?;
            opened.events = Some(check(
                "QueryInterface(IMFMediaEventGenerator)",
                opened.transform.cast::<IMFMediaEventGenerator>(),
            )?);
        }
        Ok(opened)
    }

    /// Its friendly name (`MFT_FRIENDLY_NAME_Attribute`).
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// Whether it is a hardware transform ([`is_hardware`]).
    pub(crate) fn is_hardware(&self) -> bool {
        self.hardware
    }

    /// Sets the output type, then the input type, and begins streaming.
    pub(crate) fn set_types(
        &mut self,
        output: &IMFMediaType,
        input: &IMFMediaType,
    ) -> Result<(), MediaError> {
        // SAFETY: plain COM calls on live interfaces.
        unsafe {
            check(
                "IMFTransform::SetOutputType",
                self.transform.SetOutputType(0, output, 0),
            )?;
            check(
                "IMFTransform::SetInputType",
                self.transform.SetInputType(0, input, 0),
            )?;
            self.output_kind = Some((
                check(
                    "GetGUID(MF_MT_MAJOR_TYPE)",
                    output.GetGUID(&MF_MT_MAJOR_TYPE),
                )?,
                check("GetGUID(MF_MT_SUBTYPE)", output.GetGUID(&MF_MT_SUBTYPE))?,
            ));
        }
        self.read_output_info()?;
        // SAFETY: as above.
        unsafe {
            check(
                "ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING)",
                self.transform
                    .ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0),
            )?;
            check(
                "ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM)",
                self.transform
                    .ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0),
            )?;
        }
        self.streaming = true;
        Ok(())
    }

    /// Its `ICodecAPI`, when it has one.
    pub(crate) fn codec_api(&self) -> Option<ICodecAPI> {
        self.transform.cast::<ICodecAPI>().ok()
    }

    /// Its current output type.
    pub(crate) fn output_type(&self) -> Result<IMFMediaType, MediaError> {
        // SAFETY: a plain COM call on a live interface.
        check("IMFTransform::GetOutputCurrentType", unsafe {
            self.transform.GetOutputCurrentType(0)
        })
    }

    /// Feeds it `sample`: the output samples ready after it.
    pub(crate) fn push(&mut self, sample: &IMFSample) -> Result<Vec<IMFSample>, MediaError> {
        self.check_streaming()?;
        let mut outputs = Vec::new();
        if let Some(events) = self.events.clone() {
            while self.need_input == 0 {
                let event = wait_for(|| next_event(&events, &self.name), STALL, &self.name)?;
                self.answer(event, &mut outputs)?;
            }
            // SAFETY: a plain COM call; the transform asked for this input.
            check("IMFTransform::ProcessInput", unsafe {
                self.transform.ProcessInput(0, sample, 0)
            })?;
            self.need_input -= 1;
            // What it answers at once, without waiting.
            while let Some(event) = next_event(&events, &self.name)? {
                self.answer(event, &mut outputs)?;
            }
        } else {
            loop {
                // SAFETY: a plain COM call on a live interface.
                match unsafe { self.transform.ProcessInput(0, sample, 0) } {
                    Ok(()) => break,
                    Err(error) if error.code() == MF_E_NOTACCEPTING => {
                        let before = outputs.len();
                        self.pull_all(&mut outputs)?;
                        if outputs.len() == before {
                            return Err(MediaError::Codec(format!(
                                "{} takes no input and gives no output",
                                self.name
                            )));
                        }
                    }
                    Err(error) => return Err(failed("IMFTransform::ProcessInput", &error)),
                }
            }
            self.pull_all(&mut outputs)?;
        }
        Ok(outputs)
    }

    /// Ends its input: every output sample still in it.
    pub(crate) fn drain(&mut self) -> Result<Vec<IMFSample>, MediaError> {
        self.check_streaming()?;
        self.streaming = false;
        let mut outputs = Vec::new();
        // SAFETY: plain COM calls on a live interface.
        unsafe {
            check(
                "ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM)",
                self.transform
                    .ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0),
            )?;
            check(
                "ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN)",
                self.transform.ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0),
            )?;
        }
        if let Some(events) = self.events.clone() {
            loop {
                let event = wait_for(|| next_event(&events, &self.name), STALL, &self.name)?;
                if matches!(event, Event::DrainComplete) {
                    break;
                }
                self.answer(event, &mut outputs)?;
            }
        } else {
            self.pull_all(&mut outputs)?;
        }
        Ok(outputs)
    }

    fn check_streaming(&self) -> Result<(), MediaError> {
        if self.streaming {
            Ok(())
        } else {
            Err(MediaError::Codec(format!(
                "{} is not streaming: its types are not set, or it was drained",
                self.name
            )))
        }
    }

    /// Reads whether it provides its output samples, and their size.
    fn read_output_info(&mut self) -> Result<(), MediaError> {
        // SAFETY: a plain COM call on a live interface.
        let info = check("IMFTransform::GetOutputStreamInfo", unsafe {
            self.transform.GetOutputStreamInfo(0)
        })?;
        let provides = info.dwFlags & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 != 0;
        // One that only can provide them does when it gives no size.
        let can_provide = info.dwFlags & MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES.0 as u32 != 0;
        self.provides = provides || (can_provide && info.cbSize == 0);
        self.output_size = info.cbSize;
        Ok(())
    }

    /// Acts on an asynchronous transform's `event`.
    fn answer(&mut self, event: Event, outputs: &mut Vec<IMFSample>) -> Result<(), MediaError> {
        match event {
            Event::NeedInput => self.need_input += 1,
            Event::HaveOutput => outputs.extend(self.pull()?),
            Event::DrainComplete | Event::Other => {}
        }
        Ok(())
    }

    /// Every output sample a synchronous transform has ready.
    fn pull_all(&mut self, outputs: &mut Vec<IMFSample>) -> Result<(), MediaError> {
        while let Some(sample) = self.pull()? {
            outputs.push(sample);
        }
        Ok(())
    }

    /// One `ProcessOutput`: a sample, or none when it needs more input. A
    /// stream change renegotiates the output type and asks again, up to
    /// [`STREAM_CHANGES`] times in a row.
    fn pull(&mut self) -> Result<Option<IMFSample>, MediaError> {
        let mut changes = 0;
        loop {
            let allocated = if self.provides {
                None
            } else {
                // SAFETY: plain COM calls; the sample holds the buffer.
                unsafe {
                    let buffer = check(
                        "MFCreateMemoryBuffer",
                        MFCreateMemoryBuffer(self.output_size.max(1)),
                    )?;
                    let sample = check("MFCreateSample", MFCreateSample())?;
                    check("IMFSample::AddBuffer", sample.AddBuffer(&buffer))?;
                    Some(sample)
                }
            };
            let mut buffers = [MFT_OUTPUT_DATA_BUFFER {
                dwStreamID: 0,
                pSample: ManuallyDrop::new(allocated),
                dwStatus: 0,
                pEvents: ManuallyDrop::new(None),
            }];
            let mut status = 0;
            // SAFETY: the buffer array is valid for the call; afterwards the
            // sample and the event collection it holds are taken out of
            // their `ManuallyDrop`s once, so each reference is released
            // once.
            let result = unsafe { self.transform.ProcessOutput(0, &mut buffers, &mut status) };
            let [buffer] = buffers;
            let sample = ManuallyDrop::into_inner(buffer.pSample);
            drop(ManuallyDrop::into_inner(buffer.pEvents));
            match result {
                Ok(()) => return Ok(sample),
                Err(error) if error.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(None),
                Err(error) if error.code() == MF_E_TRANSFORM_STREAM_CHANGE => {
                    count_stream_change(&mut changes, &self.name)?;
                    self.renegotiate()?;
                }
                Err(error) => return Err(failed("IMFTransform::ProcessOutput", &error)),
            }
        }
    }

    /// Sets the output type again after a stream change: the first one
    /// available, which must keep the subtype, with a video type's BT.709
    /// limited colour, which every output type carries.
    fn renegotiate(&mut self) -> Result<(), MediaError> {
        let Some((major, subtype)) = self.output_kind else {
            return Err(MediaError::Codec(format!(
                "{} changed its stream before its types were set",
                self.name
            )));
        };
        // SAFETY: plain COM calls on live interfaces.
        unsafe {
            let available = check(
                "IMFTransform::GetOutputAvailableType",
                self.transform.GetOutputAvailableType(0, 0),
            )?;
            let offered = check("GetGUID(MF_MT_SUBTYPE)", available.GetGUID(&MF_MT_SUBTYPE))?;
            if offered != subtype {
                return Err(MediaError::Codec(format!(
                    "{} changed its output subtype",
                    self.name
                )));
            }
            if major == MFMediaType_Video {
                set_color(&available)?;
            }
            check(
                "IMFTransform::SetOutputType",
                self.transform.SetOutputType(0, &available, 0),
            )?;
        }
        self.read_output_info()
    }
}

/// The next event of the asynchronous transform `name`'s `events`, or none
/// yet.
fn next_event(events: &IMFMediaEventGenerator, name: &str) -> Result<Option<Event>, MediaError> {
    // SAFETY: plain COM calls on live interfaces.
    unsafe {
        let event = match events.GetEvent(MF_EVENT_FLAG_NO_WAIT) {
            Ok(event) => event,
            Err(error) if error.code() == MF_E_NO_EVENTS_AVAILABLE => return Ok(None),
            Err(error) => return Err(failed("IMFMediaEventGenerator::GetEvent", &error)),
        };
        let kind = check("IMFMediaEvent::GetType", event.GetType())?;
        let status = check("IMFMediaEvent::GetStatus", event.GetStatus())?;
        if kind == MEError.0 as u32 || status.is_err() {
            return Err(MediaError::Codec(format!(
                "{name}'s event {kind} failed: 0x{:08X}",
                status.0 as u32
            )));
        }
        Ok(Some(if kind == METransformNeedInput.0 as u32 {
            Event::NeedInput
        } else if kind == METransformHaveOutput.0 as u32 {
            Event::HaveOutput
        } else if kind == METransformDrainComplete.0 as u32 {
            Event::DrainComplete
        } else {
            Event::Other
        }))
    }
}

impl Drop for Transform {
    /// Shuts an asynchronous transform down, as such a transform asks, then
    /// the object its activation made.
    fn drop(&mut self) {
        if self.events.is_some()
            && let Ok(shutdown) = self.transform.cast::<IMFShutdown>()
        {
            // SAFETY: a plain COM call; a failure leaves nothing to undo.
            let _ = unsafe { shutdown.Shutdown() };
        }
        // SAFETY: as above.
        let _ = unsafe { self.activate.ShutdownObject() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_media::MediaError;
    use std::time::Duration;

    #[test]
    fn a_silent_transform_stalls_out() {
        let answer = wait_for(|| Ok(None::<()>), Duration::from_millis(20), "Silent MFT");
        match answer {
            Err(MediaError::Codec(message)) => {
                assert!(message.contains("stopped answering"), "{message}");
                assert!(message.contains("Silent MFT"), "{message}");
            }
            other => panic!("{other:?}"),
        }
        // An answer ends the wait.
        let mut polls = 0;
        let answer = wait_for(
            || {
                polls += 1;
                Ok((polls == 3).then_some(polls))
            },
            Duration::from_secs(5),
            "Slow MFT",
        );
        assert_eq!(answer.unwrap(), 3);
        // A failure ends it too.
        let failed: Result<(), MediaError> = wait_for(
            || Err(MediaError::Codec("GetEvent failed: 0x80004005".into())),
            Duration::from_secs(5),
            "Broken MFT",
        );
        assert!(matches!(failed, Err(MediaError::Codec(message)) if message.contains("GetEvent")));
    }

    #[test]
    fn a_refusing_candidate_falls_back() {
        let candidates = ["first", "second", "third"];
        let mut tried = Vec::new();
        let opened = first_accepting("h264", &candidates, |name| {
            tried.push(*name);
            if *name == "third" {
                Ok(format!("{name}'s session"))
            } else {
                Err(MediaError::Codec(format!("{name} refused")))
            }
        });
        assert_eq!(opened.unwrap(), "third's session");
        assert_eq!(tried, candidates);

        // None accepts: the component is unavailable, every refusal said.
        let refused: Result<(), MediaError> = first_accepting("h264", &candidates, |name| {
            Err(MediaError::Codec(format!("{name} refused")))
        });
        assert!(
            matches!(&refused, Err(MediaError::ComponentUnavailable { component, reason })
                if component == "h264"
                    && reason == "every transform Media Foundation lists refused: first refused; second refused; third refused"),
            "{refused:?}"
        );
        let none: Result<(), MediaError> = first_accepting("aac", &[] as &[&str], |_| Ok(()));
        assert!(
            matches!(&none, Err(MediaError::ComponentUnavailable { component, reason })
                if component == "aac" && reason == "Media Foundation listed no transform to try"),
            "{none:?}"
        );

        // The first that accepts wins; the rest are not tried.
        let mut tried = 0;
        let opened = first_accepting("h264", &candidates, |name| {
            tried += 1;
            Ok(*name)
        });
        assert_eq!((opened.unwrap(), tried), ("first", 1));
    }

    /// A refusal of the rate control, as `video.rs` words it.
    fn no_quality_mode(name: &str) -> MediaError {
        MediaError::InvalidSettings {
            key: "rateControl".to_owned(),
            message: format!("{name} has no constant quality mode; choose Average bitrate"),
        }
    }

    #[test]
    fn a_rate_control_refusal_outranks_later_refusals() {
        // A GPU's MFT refuses the rate control, a DX12 one its output type:
        // the user is told what to change, not the DX12 error.
        let candidates = ["GPU MFT", "DX12 MFT"];
        let refused: Result<(), MediaError> = first_accepting("h264", &candidates, |name| {
            if *name == "GPU MFT" {
                Err(no_quality_mode(name))
            } else {
                Err(MediaError::Codec(format!(
                    "{name}: IMFTransform::SetOutputType failed: 0x80041000"
                )))
            }
        });
        assert!(
            matches!(&refused, Err(MediaError::InvalidSettings { key, message })
                if key == "rateControl" && message.starts_with("GPU MFT has no constant quality mode")),
            "{refused:?}"
        );
        // A later settings refusal replaces an earlier one, as any refusal does.
        let refused: Result<(), MediaError> =
            first_accepting("h264", &candidates, |name| Err(no_quality_mode(name)));
        assert!(
            matches!(&refused, Err(MediaError::InvalidSettings { message, .. }) if message.starts_with("DX12 MFT")),
            "{refused:?}"
        );
    }

    #[test]
    fn a_rate_control_refusal_falls_back() {
        // Deviation 78: the next candidate (Microsoft's software MFT) opens.
        let candidates = ["GPU MFT", "H264 Encoder MFT"];
        let opened = first_accepting("h264", &candidates, |name| {
            if *name == "GPU MFT" {
                Err(no_quality_mode(name))
            } else {
                Ok(format!("{name}'s session"))
            }
        });
        assert_eq!(opened.unwrap(), "H264 Encoder MFT's session");
    }

    #[test]
    fn stream_changes_in_a_row_are_capped() {
        let mut changes = 0;
        for _ in 0..STREAM_CHANGES {
            count_stream_change(&mut changes, "Restless MFT").unwrap();
        }
        assert!(matches!(
            count_stream_change(&mut changes, "Restless MFT"),
            Err(MediaError::Codec(message)) if message == "Restless MFT kept changing its output type"
        ));
    }

    #[test]
    fn refusals_name_their_transform() {
        assert!(matches!(
            named("NVIDIA H.264 Encoder MFT", MediaError::Codec("IMFTransform::SetOutputType failed: 0xC00D6D76".into())),
            MediaError::Codec(message) if message == "NVIDIA H.264 Encoder MFT: IMFTransform::SetOutputType failed: 0xC00D6D76"
        ));
        // A message that leads with the name already is left alone.
        assert!(matches!(
            named("Silent MFT", MediaError::Codec("Silent MFT stopped answering".into())),
            MediaError::Codec(message) if message == "Silent MFT stopped answering"
        ));
        assert!(matches!(
            named("GPU MFT", no_quality_mode("GPU MFT")),
            MediaError::InvalidSettings { key, .. } if key == "rateControl"
        ));
    }
}
