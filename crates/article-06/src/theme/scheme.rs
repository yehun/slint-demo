#![allow(non_snake_case)]

use serde::{Deserialize, Serialize};
use super::utils::string_to_color;
use crate::MaterialScheme;
use crate::MaterialSchemes;

/// JSON 反序列化用的色彩方案 (字段为 hex 字符串)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JsonScheme {
    pub primary: String,
    pub surfaceTint: String,
    pub onPrimary: String,
    pub primaryContainer: String,
    pub onPrimaryContainer: String,
    pub secondary: String,
    pub onSecondary: String,
    pub secondaryContainer: String,
    pub onSecondaryContainer: String,
    pub tertiary: String,
    pub onTertiary: String,
    pub tertiaryContainer: String,
    pub onTertiaryContainer: String,
    pub error: String,
    pub onError: String,
    pub errorContainer: String,
    pub onErrorContainer: String,
    pub background: String,
    pub onBackground: String,
    pub surface: String,
    pub onSurface: String,
    pub surfaceVariant: String,
    pub onSurfaceVariant: String,
    pub outline: String,
    pub outlineVariant: String,
    pub shadow: String,
    pub scrim: String,
    pub inverseSurface: String,
    pub inverseOnSurface: String,
    pub inversePrimary: String,
    pub primaryFixed: String,
    pub onPrimaryFixed: String,
    pub primaryFixedDim: String,
    pub onPrimaryFixedVariant: String,
    pub secondaryFixed: String,
    pub onSecondaryFixed: String,
    pub secondaryFixedDim: String,
    pub onSecondaryFixedVariant: String,
    pub tertiaryFixed: String,
    pub onTertiaryFixed: String,
    pub tertiaryFixedDim: String,
    pub onTertiaryFixedVariant: String,
    pub surfaceDim: String,
    pub surfaceBright: String,
    pub surfaceContainerLowest: String,
    pub surfaceContainerLow: String,
    pub surfaceContainer: String,
    pub surfaceContainerHigh: String,
    pub surfaceContainerHighest: String,
}

impl JsonScheme {
    /// 转换为 Slint 生成的 MaterialScheme 结构体 (字段名为 camelCase)
    pub fn to_slint(&self) -> MaterialScheme {
        MaterialScheme {
            primary: string_to_color(self.primary.clone()),
            surfaceTint: string_to_color(self.surfaceTint.clone()),
            onPrimary: string_to_color(self.onPrimary.clone()),
            primaryContainer: string_to_color(self.primaryContainer.clone()),
            onPrimaryContainer: string_to_color(self.onPrimaryContainer.clone()),
            secondary: string_to_color(self.secondary.clone()),
            onSecondary: string_to_color(self.onSecondary.clone()),
            secondaryContainer: string_to_color(self.secondaryContainer.clone()),
            onSecondaryContainer: string_to_color(self.onSecondaryContainer.clone()),
            tertiary: string_to_color(self.tertiary.clone()),
            onTertiary: string_to_color(self.onTertiary.clone()),
            tertiaryContainer: string_to_color(self.tertiaryContainer.clone()),
            onTertiaryContainer: string_to_color(self.onTertiaryContainer.clone()),
            error: string_to_color(self.error.clone()),
            onError: string_to_color(self.onError.clone()),
            errorContainer: string_to_color(self.errorContainer.clone()),
            onErrorContainer: string_to_color(self.onErrorContainer.clone()),
            background: string_to_color(self.background.clone()),
            onBackground: string_to_color(self.onBackground.clone()),
            surface: string_to_color(self.surface.clone()),
            onSurface: string_to_color(self.onSurface.clone()),
            surfaceVariant: string_to_color(self.surfaceVariant.clone()),
            onSurfaceVariant: string_to_color(self.onSurfaceVariant.clone()),
            outline: string_to_color(self.outline.clone()),
            outlineVariant: string_to_color(self.outlineVariant.clone()),
            shadow: string_to_color(self.shadow.clone()),
            scrim: string_to_color(self.scrim.clone()),
            inverseSurface: string_to_color(self.inverseSurface.clone()),
            inverseOnSurface: string_to_color(self.inverseOnSurface.clone()),
            inversePrimary: string_to_color(self.inversePrimary.clone()),
            primaryFixed: string_to_color(self.primaryFixed.clone()),
            onPrimaryFixed: string_to_color(self.onPrimaryFixed.clone()),
            primaryFixedDim: string_to_color(self.primaryFixedDim.clone()),
            onPrimaryFixedVariant: string_to_color(self.onPrimaryFixedVariant.clone()),
            secondaryFixed: string_to_color(self.secondaryFixed.clone()),
            onSecondaryFixed: string_to_color(self.onSecondaryFixed.clone()),
            secondaryFixedDim: string_to_color(self.secondaryFixedDim.clone()),
            onSecondaryFixedVariant: string_to_color(self.onSecondaryFixedVariant.clone()),
            tertiaryFixed: string_to_color(self.tertiaryFixed.clone()),
            onTertiaryFixed: string_to_color(self.onTertiaryFixed.clone()),
            tertiaryFixedDim: string_to_color(self.tertiaryFixedDim.clone()),
            onTertiaryFixedVariant: string_to_color(self.onTertiaryFixedVariant.clone()),
            surfaceDim: string_to_color(self.surfaceDim.clone()),
            surfaceBright: string_to_color(self.surfaceBright.clone()),
            surfaceContainerLowest: string_to_color(self.surfaceContainerLowest.clone()),
            surfaceContainerLow: string_to_color(self.surfaceContainerLow.clone()),
            surfaceContainer: string_to_color(self.surfaceContainer.clone()),
            surfaceContainerHigh: string_to_color(self.surfaceContainerHigh.clone()),
            surfaceContainerHighest: string_to_color(self.surfaceContainerHighest.clone()),
        }
    }
}

/// JSON 反序列化用的色彩方案集 (light + dark)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JsonSchemes {
    pub dark: JsonScheme,
    pub light: JsonScheme,
}

impl JsonSchemes {
    /// 转换为 Slint 生成的 MaterialSchemes 结构体
    pub fn to_slint(&self) -> MaterialSchemes {
        MaterialSchemes {
            dark: self.dark.to_slint(),
            light: self.light.to_slint(),
        }
    }
}
