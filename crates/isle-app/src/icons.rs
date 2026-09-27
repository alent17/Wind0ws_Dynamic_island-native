//! The same Lucide SVG paths shipped by the Svelte UI, rendered by Direct2D.
use windows::{
    core::*,
    Foundation::Numerics::Matrix3x2,
    Win32::{
        Graphics::Direct2D::{Common::*, *},
        UI::Shell::SHCreateMemStream,
    },
};

#[derive(Clone, Copy)]
pub enum Icon {
    Timer,
    Volume,
    Floating,
    Settings,
    Hide,
    Clock,
    CloudSun,
    Back,
    Close,
    Check,
    Play,
    Pause,
    ChevronDown,
    Sun,
    Cloud,
    Fog,
    Rain,
    Snow,
    Lightning,
    ChevronUp,
    Music,
}

pub struct Icons {
    context: ID2D1DeviceContext5,
    documents: Vec<(ID2D1SvgDocument, ID2D1SvgElement)>,
}
impl Icons {
    pub unsafe fn new(context: &ID2D1DeviceContext) -> Result<Self> {
        let context: ID2D1DeviceContext5 = context.cast()?;
        macro_rules! asset {
            ($name:literal) => {
                include_bytes!(concat!("../../../assets/icons/", $name, ".svg")).as_slice()
            };
        }
        let sources = [
            asset!("timer"),
            asset!("volume-2"),
            asset!("gallery-horizontal-end"),
            asset!("settings"),
            asset!("eye-off"),
            asset!("clock"),
            asset!("cloud-sun"),
            asset!("arrow-left"),
            asset!("x"),
            asset!("check"),
            asset!("play"),
            asset!("pause"),
            asset!("chevron-down"),
            asset!("sun"),
            asset!("cloud"),
            asset!("cloud-fog"),
            asset!("cloud-rain"),
            asset!("cloud-snow"),
            asset!("cloud-lightning"),
            asset!("chevron-up"),
            asset!("music-2"),
        ];
        let documents = sources
            .into_iter()
            .map(|source| {
                let stream = SHCreateMemStream(Some(source)).ok_or_else(Error::from_win32)?;
                let doc = context.CreateSvgDocument(
                    &stream,
                    D2D_SIZE_F {
                        width: 24.,
                        height: 24.,
                    },
                )?;
                let root = doc.GetRoot()?;
                Ok((doc, root))
            })
            .collect::<Result<_>>()?;
        Ok(Self { context, documents })
    }
    pub unsafe fn draw(
        &self,
        icon: Icon,
        x: f32,
        y: f32,
        size: f32,
        stroke: f32,
        color: D2D1_COLOR_F,
    ) -> Result<()> {
        let (document, root) = &self.documents[icon as usize];
        root.SetAttributeValue2(
            w!("color"),
            D2D1_SVG_ATTRIBUTE_POD_TYPE_COLOR,
            (&color as *const D2D1_COLOR_F).cast(),
            std::mem::size_of::<D2D1_COLOR_F>() as u32,
        )?;
        root.SetAttributeValue2(
            w!("stroke-width"),
            D2D1_SVG_ATTRIBUTE_POD_TYPE_FLOAT,
            (&stroke as *const f32).cast(),
            4,
        )?;
        root.SetAttributeValue2(
            w!("opacity"),
            D2D1_SVG_ATTRIBUTE_POD_TYPE_FLOAT,
            (&color.a as *const f32).cast(),
            4,
        )?;
        let mut original = Matrix3x2::default();
        self.context.GetTransform(&mut original);
        self.context.SetTransform(&Matrix3x2 {
            M11: size / 24.,
            M12: 0.,
            M21: 0.,
            M22: size / 24.,
            M31: x,
            M32: y,
        });
        self.context.DrawSvgDocument(document);
        self.context.SetTransform(&original);
        Ok(())
    }
}

pub fn weather(code: Option<u32>) -> Icon {
    match code {
        Some(0) => Icon::Sun,
        Some(1..=3) => Icon::CloudSun,
        Some(4..=48) => Icon::Fog,
        Some(49..=67) | Some(78..=86) => Icon::Rain,
        Some(68..=77) => Icon::Snow,
        Some(_) => Icon::Lightning,
        None => Icon::Cloud,
    }
}
