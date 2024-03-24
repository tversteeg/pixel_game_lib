//! Show how a sprite can be loaded from disk and rendered multiple times..

use glamour::{Size2, Vector2};
use pixel_game_lib::{Context, KeyCode, MouseButton, PixelGame, WindowConfig};

/// A single sprite instance to draw.
struct Sprite {
    /// Absolute position in pixels on the buffer.
    position: Vector2,
    /// Rotation in radians.
    rotation: f32,
}

/// Define a game state for our example.
#[derive(Default)]
struct GameState {
    /// Sprites to draw.
    sprites: Vec<Sprite>,
}

impl PixelGame for GameState {
    // Update and render the game
    fn tick(&mut self, ctx: Context) {
        // Exit when escape is pressed
        if ctx.key_pressed(KeyCode::Escape) {
            ctx.exit();

            return;
        }

        // If the left mouse button is pressed add a new sprite
        if let Some(mouse) = ctx.mouse() {
            if ctx.mouse_pressed(MouseButton::Left) {
                // Spawn a new sprite in the render loop
                self.sprites.push(Sprite {
                    position: mouse,
                    rotation: 0.0,
                });
            }
        }

        // If the right mouse button is held rotate every sprite a tiny bit
        if ctx.mouse_held(MouseButton::Right) {
            self.sprites
                .iter_mut()
                .for_each(|sprite| sprite.rotation += ctx.delta_time());
        }

        // Draw sprite, will be loaded from disk if the `hot-reloading` feature is enabled, otherwise it will be embedded in the binary
        for sprite in &self.sprites {
            ctx.draw_sprite_rotated("threeforms", sprite.position, sprite.rotation);
        }

        // Draw a basic FPS counter
        let fps = ctx.delta_time().recip();
        ctx.draw_text("Beachball", Vector2::ZERO, format!("{fps:.1}"));
        ctx.draw_text(
            "Beachball",
            Vector2::new(0.0, 240.0 - 20.0),
            "Left mouse: new sprite\nRight mouse: rotate",
        );
    }
}

/// Open an empty window.
fn main() {
    // Window configuration with huge pixels
    let window_config = WindowConfig {
        buffer_size: Size2::new(320.0, 240.0),
        scaling: 3.0,
        ..Default::default()
    };

    // Spawn the window and run the 'game'
    GameState::default()
        .run(window_config)
        .expect("Error running game");
}
