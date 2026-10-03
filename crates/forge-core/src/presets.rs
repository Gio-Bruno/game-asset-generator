use crate::{
    contract::{AssetKind, StyleGuide, StylePreset},
    error::{ApiError, Result},
};

pub fn all() -> Vec<StylePreset> {
    [
        ("woodland","Woodland ink","Warm, storybook worlds","Hand-painted 2D game art. Rounded readable silhouettes, delicate forest-green outlines, subtle paper texture, warm amber accents and restrained shading.",["#315C4B","#D7AD70","#F1E9D5"],512),
        ("pixel","Pixel adventure","Crisp, nostalgic sprites","Pixel art for a 2D game. Deliberate square pixels, compact readable silhouettes, limited palette, hard edges, no anti-aliasing or smooth gradients. Consistent pixel grid and character scale.",["#253B50","#6AA778","#E7BD76"],256),
        ("flat","Bold & playful","Graphic shapes, clear color","Flat 2D game illustration. Bold geometric shapes, strong silhouettes, clean edges, limited solid colors, minimal shading, friendly exaggerated proportions.",["#173B3F","#E98254","#F3CC61"],512),
        ("ink","Sketchbook","Expressive lines, soft color","Hand-drawn 2D game art. Expressive ink contours, understated watercolor washes, warm paper tones, consistent fine line weight, subtle texture and clean readable shapes.",["#313637","#A86656","#EDE2CA"],512),
        ("paint","Painterly fantasy","Rich, atmospheric settings","Painterly 2D fantasy game art. Cohesive brush texture, clear silhouette hierarchy, detailed materials, muted earth colors and cinematic soft light. Keep characters readable at game scale.",["#344948","#867756","#C5AF84"],1024),
        ("isometric","Tiny isometric","Cozy, modular worlds","Isometric 2D game art. Consistent 30-degree isometric camera, orthographic projection, clean dimensional shapes, soft ambient occlusion, stylized materials and modular game-friendly composition.",["#4B6563","#C19165","#DAD7BB"],512),
    ].into_iter().map(|(id,title,subtitle,description,palette,size)|StylePreset{
        id:id.into(),title:title.into(),subtitle:subtitle.into(),style:StyleGuide{preset_id:Some(id.into()),name:title.into(),description:description.into(),palette:palette.into_iter().map(String::from).collect(),perspective:if id=="isometric"{"30-degree isometric, orthographic"}else{"Side view, orthographic"}.into(),lighting:"Soft light from upper left; consistent across all assets".into(),reference_asset_ids:vec![]},character_size:size,scene_width:1536,scene_height:1024,
    }).collect()
}
pub fn get(id: &str) -> Result<StylePreset> {
    all()
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| ApiError::validation("Choose a known style preset."))
}

/// Legacy style documents infer their starter preset from the original title.
pub fn selected_id(style: &StyleGuide) -> Option<String> {
    style.preset_id.clone().or_else(|| {
        all()
            .into_iter()
            .find(|p| p.title == style.name)
            .map(|p| p.id)
    })
}
/// One source of defaults for the native composer and art director.
pub fn output_size(style: &StyleGuide, kind: AssetKind) -> (u32, u32) {
    let preset = selected_id(style).and_then(|id| get(&id).ok());
    if matches!(kind, AssetKind::Scene | AssetKind::ConceptSheet) {
        preset
            .map(|p| (p.scene_width, p.scene_height))
            .unwrap_or((1536, 1024))
    } else {
        let side = preset.map(|p| p.character_size).unwrap_or_else(|| {
            if style.description.to_lowercase().contains("pixel art") {
                256
            } else {
                512
            }
        });
        (side, side)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_customized_presets_keep_their_defaults() {
        let mut style = get("paint").unwrap().style;
        style.name = "My moonlit world".into();
        style.palette = vec!["#7386A4".into()];
        let restored: StyleGuide =
            serde_json::from_value(serde_json::to_value(style).unwrap()).unwrap();
        assert_eq!(output_size(&restored, AssetKind::Character), (1024, 1024));
        assert_eq!(output_size(&restored, AssetKind::Scene), (1536, 1024));
        let legacy: StyleGuide = serde_json::from_value(
            serde_json::json!({"name":"Pixel adventure","description":"Pixel art"}),
        )
        .unwrap();
        assert_eq!(output_size(&legacy, AssetKind::Character), (256, 256));
        assert_eq!(
            output_size(&StyleGuide::default(), AssetKind::Prop),
            (512, 512)
        );
    }
}
