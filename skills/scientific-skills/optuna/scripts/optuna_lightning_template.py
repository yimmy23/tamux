"""
Optuna + PyTorch Lightning Training Template
Complete hyperparameter optimization workflow for DL training.
"""
import optuna
import pytorch_lightning as pl
from pytorch_lightning.callbacks import EarlyStopping, ModelCheckpoint
from optuna.integration import PyTorchLightningPruningCallback
import torch
from torch import nn
from torch.utils.data import DataLoader, TensorDataset


class TemplateLightningModule(pl.LightningModule):
    """Replace with your actual model."""

    def __init__(self, lr: float, hidden_dim: int, n_layers: int, dropout: float):
        super().__init__()
        self.save_hyperparameters()
        layers = []
        in_dim = 784  # Example: MNIST input
        for _ in range(n_layers):
            layers.extend(
                [
                    nn.Linear(in_dim, hidden_dim),
                    nn.ReLU(),
                    nn.Dropout(dropout),
                ]
            )
            in_dim = hidden_dim
        layers.append(nn.Linear(hidden_dim, 10))
        self.model = nn.Sequential(*layers)
        self.loss_fn = nn.CrossEntropyLoss()

    def forward(self, x):
        return self.model(x.view(x.size(0), -1))

    def training_step(self, batch, batch_idx):
        x, y = batch
        logits = self(x)
        loss = self.loss_fn(logits, y)
        acc = (logits.argmax(1) == y).float().mean()
        self.log("train_loss", loss, prog_bar=True)
        self.log("train_acc", acc, prog_bar=True)
        return loss

    def validation_step(self, batch, batch_idx):
        x, y = batch
        logits = self(x)
        loss = self.loss_fn(logits, y)
        acc = (logits.argmax(1) == y).float().mean()
        self.log("val_loss", loss, prog_bar=True)
        self.log("val_acc", acc, prog_bar=True)
        return loss

    def configure_optimizers(self):
        optimizer = torch.optim.AdamW(self.parameters(), lr=self.hparams.lr)
        scheduler = torch.optim.lr_scheduler.CosineAnnealingLR(
            optimizer, T_max=self.trainer.max_epochs
        )
        return [optimizer], [scheduler]


def get_dataloaders(batch_size: int):
    """Replace with your actual data loading."""
    from torchvision import datasets, transforms

    transform = transforms.Compose([transforms.ToTensor(), transforms.Normalize((0.1307,), (0.3081,))])
    train_ds = datasets.MNIST("./data", train=True, download=True, transform=transform)
    val_ds = datasets.MNIST("./data", train=False, download=True, transform=transform)

    train_loader = DataLoader(train_ds, batch_size=batch_size, shuffle=True, num_workers=4)
    val_loader = DataLoader(val_ds, batch_size=batch_size, shuffle=False, num_workers=4)
    return train_loader, val_loader


def objective(trial: optuna.Trial) -> float:
    """Optuna objective function for hyperparameter optimization."""
    # --- Hyperparameter suggestions ---
    lr = trial.suggest_float("lr", 1e-5, 1e-2, log=True)
    hidden_dim = trial.suggest_int("hidden_dim", 64, 1024, step=64)
    n_layers = trial.suggest_int("n_layers", 1, 6)
    dropout = trial.suggest_float("dropout", 0.0, 0.5)
    batch_size = trial.suggest_categorical("batch_size", [32, 64, 128, 256])

    # --- Data ---
    train_loader, val_loader = get_dataloaders(batch_size)

    # --- Model ---
    model = TemplateLightningModule(lr=lr, hidden_dim=hidden_dim, n_layers=n_layers, dropout=dropout)

    # --- Callbacks ---
    pruning_callback = PyTorchLightningPruningCallback(trial, monitor="val_loss")
    early_stop = EarlyStopping(monitor="val_loss", patience=10, mode="min")
    checkpoint = ModelCheckpoint(monitor="val_loss", mode="min", save_top_k=1)

    # --- Trainer ---
    trainer = pl.Trainer(
        max_epochs=50,
        accelerator="auto",
        devices=1,
        callbacks=[pruning_callback, early_stop, checkpoint],
        enable_progress_bar=False,
        logger=False,
        enable_model_summary=False,
    )

    trainer.fit(model, train_loader, val_loader)

    # Return best validation loss
    return trainer.callback_metrics.get("val_loss", float("inf")).item()


def run_study(n_trials: int = 50, storage: str = None, study_name: str = "lightning_study"):
    """Run the Optuna study with optional distributed storage."""
    sampler = optuna.samplers.TPESampler(seed=42, n_startup_trials=10)
    pruner = optuna.pruners.MedianPruner(n_startup_trials=5, n_warmup_steps=10)

    study = optuna.create_study(
        study_name=study_name,
        direction="minimize",
        sampler=sampler,
        pruner=pruner,
        storage=storage,
        load_if_exists=True,
    )

    study.optimize(objective, n_trials=n_trials, n_jobs=1)  # Set n_jobs > 1 for parallel

    print("=" * 60)
    print("Best trial:")
    print(f"  Value (val_loss): {study.best_value:.4f}")
    print(f"  Params: {study.best_params}")
    print("=" * 60)

    # Save results
    import json

    with open("optuna_best_params.json", "w") as f:
        json.dump(study.best_params, f, indent=2)

    return study


if __name__ == "__main__":
    run_study(n_trials=50)
