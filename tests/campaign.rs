use std::{env, fs, path::Path};
use ts2::{
    campaign,
    formats::{exe, leveldat, ngn, packets, terrain},
};

#[test]
fn campaign_paths_keep_original_script_ids() {
    let game = Path::new("game");
    let mut ids = std::collections::BTreeSet::new();
    for level in &campaign::LEVELS {
        assert_eq!(
            exe::level_number(&level.path(game)),
            Some(level.internal_id)
        );
        assert!(ids.insert(level.internal_id));
    }
    assert_eq!(campaign::level(3).unwrap().internal_id, 6);
    assert_eq!(campaign::level(6).unwrap().internal_id, 3);
    assert_eq!(
        campaign::level(11).unwrap().path(game),
        game.join("data/level01/level1.ngn")
    );
    assert_eq!(
        campaign::level(15).unwrap().path(game),
        game.join("data/level05/level1.ngn")
    );
    assert!(campaign::level(0).is_none());
    assert!(campaign::level(16).is_none());
}

#[test]
fn all_campaign_assets_parse() {
    let Some(game) = env::var_os("TS2_GAME_DIR") else {
        return;
    };
    for level in &campaign::LEVELS {
        let path = level.path(Path::new(&game));
        let n = ngn::parse(&fs::read(&path).unwrap()).unwrap();
        assert!(!n.textures.is_empty(), "{}", path.display());
        assert!(
            n.creatures.iter().any(|c| c.id == 0),
            "Buzz missing: {}",
            path.display()
        );
        let lists = leveldat::parse(&fs::read(path.with_extension("dat")).unwrap()).unwrap();
        // Boss arenas may legitimately contain no point lists.
        let _ = lists;
        let pk = packets::parse(&fs::read(path.with_extension("raw")).unwrap()).unwrap();
        let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
        assert!(!spawns.is_empty());
        if level.number == 3 {
            assert_eq!(spawns[0].creature, 48);
        }
        if level.number == 6 {
            assert_eq!(spawns[0].creature, 16);
        }
        let t = terrain::parse(&fs::read(terrain::path_for(&path).unwrap()).unwrap()).unwrap();
        assert!(!t.objects.is_empty());
    }
}
